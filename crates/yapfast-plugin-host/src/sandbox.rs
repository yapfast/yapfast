//! The operating system's half of the helper's sandbox (PLAN.md §9.5).
//!
//! Wasmtime is the first wall: a guest has no filesystem, no sockets and no
//! host function that reaches either. This is the second, for the day a
//! wasmtime or Cranelift bug lets guest code run as the helper itself. `run`
//! needs no network and reads only the plugin store, writing only its compile
//! cache, so that is all it keeps:
//!
//! * **Landlock** limits the filesystem to the store (read), the compile cache
//!   (read and write) and the system's library folders (read). On a kernel with
//!   Landlock ABI 4 it also refuses every TCP bind and connect, and from ABI 6
//!   it can't signal processes outside itself or reach abstract Unix sockets.
//!   Nothing may be executed: no rule grants `EXECUTE`.
//! * **seccomp** refuses `socket`, `ptrace`, `process_vm_readv`/`writev`,
//!   `execve`/`execveat` and io_uring (whose socket operations seccomp can't
//!   see), with `EPERM` rather than a kill, so a library that probes for one
//!   gets an error it can handle.
//!
//! Both are applied once, from `main`'s thread, before the pipe, the log relay
//! or a worker starts a thread: Landlock binds only the calling thread and the
//! threads it starts later, which is every thread the helper will have.
//!
//! A part that can't be applied (an old kernel, an architecture without a
//! filter here, a store that isn't there) is left out and named in the
//! [`Report`]; the helper still runs under wasmtime's sandbox, and the
//! Plugins page says what is missing. Windows and macOS have no code here
//! yet; see the audit's sketch.

use std::path::{Path, PathBuf};

/// What the sandbox ended up as.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// The parts that are on, as short names: `landlock v4`, `seccomp`.
    pub applied: Vec<String>,
    /// Why a part is off, one sentence each.
    pub missing: Vec<String>,
}

impl Report {
    /// Whether every part this platform has is on.
    pub fn complete(&self) -> bool {
        !self.applied.is_empty() && self.missing.is_empty()
    }
}

/// The folders the helper may still reach once sandboxed.
#[derive(Clone, Debug, Default)]
pub struct Paths {
    /// Read only: the plugin store, and a development folder.
    pub read: Vec<PathBuf>,
    /// Read and write: the compile cache.
    pub write: Vec<PathBuf>,
}

/// Applies every part of the sandbox this platform has.
///
/// # Safety
///
/// Call it while the process has one thread. Landlock binds the calling thread
/// and its later children only, so a thread started before it would run
/// unconfined.
pub unsafe fn apply(paths: &Paths) -> Report {
    #[cfg(target_os = "linux")]
    {
        linux::apply(paths)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = paths;
        Report {
            applied: Vec::new(),
            missing: vec!["this platform has no OS sandbox for plugins yet".to_owned()],
        }
    }
}

/// Folders a dynamically linked helper may read libraries from, if they exist.
#[cfg(target_os = "linux")]
const SYSTEM_READ: &[&str] = &["/usr", "/lib", "/lib64", "/nix/store", "/etc/ld.so.cache"];

/// Files a process expects to open whatever it is: read and write.
#[cfg(target_os = "linux")]
const DEVICES: &[&str] = &["/dev/null", "/dev/urandom"];

#[cfg(target_os = "linux")]
mod linux {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    use super::{DEVICES, Path, Paths, Report, SYSTEM_READ};

    // Landlock's interface, from <linux/landlock.h>. libc names the syscalls
    // but not the structures or the access bits.
    const CREATE_RULESET_VERSION: libc::c_uint = 1;
    const RULE_PATH_BENEATH: libc::c_int = 1;

    const FS_EXECUTE: u64 = 1 << 0;
    const FS_WRITE_FILE: u64 = 1 << 1;
    const FS_READ_FILE: u64 = 1 << 2;
    const FS_READ_DIR: u64 = 1 << 3;
    const FS_REMOVE_DIR: u64 = 1 << 4;
    const FS_REMOVE_FILE: u64 = 1 << 5;
    const FS_MAKE_DIR: u64 = 1 << 7;
    const FS_MAKE_REG: u64 = 1 << 8;
    /// Every right ABI 1 knows: execute through make-symlink.
    const FS_V1: u64 = (1 << 13) - 1;
    /// ABI 2: linking or renaming a file into another folder.
    const FS_REFER: u64 = 1 << 13;
    /// ABI 3.
    const FS_TRUNCATE: u64 = 1 << 14;
    /// ABI 5.
    const FS_IOCTL_DEV: u64 = 1 << 15;
    /// The rights that make sense on a file rather than a folder.
    const FS_FILE: u64 = FS_EXECUTE | FS_WRITE_FILE | FS_READ_FILE | FS_TRUNCATE | FS_IOCTL_DEV;

    /// ABI 4: TCP.
    const NET_BIND_TCP: u64 = 1 << 0;
    const NET_CONNECT_TCP: u64 = 1 << 1;
    /// ABI 6: what a process outside the sandbox can be reached by.
    const SCOPE_ABSTRACT_UNIX_SOCKET: u64 = 1 << 0;
    const SCOPE_SIGNAL: u64 = 1 << 1;

    #[repr(C)]
    struct RulesetAttr {
        handled_access_fs: u64,
        handled_access_net: u64,
        scoped: u64,
    }

    #[repr(C, packed)]
    struct PathBeneathAttr {
        allowed_access: u64,
        parent_fd: i32,
    }

    pub(super) fn apply(paths: &Paths) -> Report {
        let mut report = Report::default();
        // Both need it, and with it set neither can be undone by exec.
        // Safety: a plain prctl with integer arguments.
        if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
            report.missing.push(format!(
                "the helper could not give up new privileges ({}), so no OS sandbox applies",
                std::io::Error::last_os_error()
            ));
            return report;
        }
        match landlock(paths) {
            Ok(abi) => report.applied.push(format!("landlock v{abi}")),
            Err(reason) => report.missing.push(reason),
        }
        match seccomp() {
            Ok(()) => report.applied.push("seccomp".to_owned()),
            Err(reason) => report.missing.push(reason),
        }
        report
    }

    /// Confines the filesystem, and the network where the kernel can, and
    /// returns the Landlock ABI it used.
    fn landlock(paths: &Paths) -> Result<i64, String> {
        if paths.read.is_empty() {
            return Err(
                "the helper was not told where the plugin store is, so its files are not confined"
                    .to_owned(),
            );
        }
        // Safety: the version query takes no attribute.
        let abi = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                std::ptr::null::<RulesetAttr>(),
                0usize,
                CREATE_RULESET_VERSION,
            )
        };
        if abi < 1 {
            return Err(format!(
                "this kernel has no Landlock ({}), so the helper's files are not confined",
                std::io::Error::last_os_error()
            ));
        }
        let mut handled = FS_V1;
        if abi >= 2 {
            handled |= FS_REFER;
        }
        if abi >= 3 {
            handled |= FS_TRUNCATE;
        }
        if abi >= 5 {
            handled |= FS_IOCTL_DEV;
        }
        let attr = RulesetAttr {
            handled_access_fs: handled,
            handled_access_net: if abi >= 4 {
                NET_BIND_TCP | NET_CONNECT_TCP
            } else {
                0
            },
            scoped: if abi >= 6 {
                SCOPE_ABSTRACT_UNIX_SOCKET | SCOPE_SIGNAL
            } else {
                0
            },
        };
        // An older kernel reads the leading fields only; the rest are zero
        // for it, which `copy_struct_from_user` accepts.
        // Safety: `attr` lives across the call and its size is passed.
        let ruleset = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                &attr as *const RulesetAttr,
                std::mem::size_of::<RulesetAttr>(),
                0u32,
            )
        };
        if ruleset < 0 {
            return Err(format!(
                "Landlock refused a ruleset ({}), so the helper's files are not confined",
                std::io::Error::last_os_error()
            ));
        }
        let ruleset = ruleset as libc::c_int;
        let result = fill_and_restrict(ruleset, handled, paths);
        // Safety: the descriptor is ours, and restricting doesn't need it open.
        unsafe { libc::close(ruleset) };
        result.map(|()| abi)
    }

    fn fill_and_restrict(ruleset: libc::c_int, handled: u64, paths: &Paths) -> Result<(), String> {
        let read = FS_READ_FILE | FS_READ_DIR;
        let write = read
            | FS_WRITE_FILE
            | FS_REMOVE_DIR
            | FS_REMOVE_FILE
            | FS_MAKE_DIR
            | FS_MAKE_REG
            | FS_REFER
            | FS_TRUNCATE;
        for path in &paths.read {
            allow(ruleset, path, read & handled, true)?;
        }
        for path in &paths.write {
            allow(ruleset, path, write & handled, true)?;
        }
        for path in SYSTEM_READ {
            allow(ruleset, Path::new(path), read & handled, false)?;
        }
        for path in DEVICES {
            allow(
                ruleset,
                Path::new(path),
                (FS_READ_FILE | FS_WRITE_FILE) & handled,
                false,
            )?;
        }
        // Safety: a descriptor we opened, and no flags.
        if unsafe { libc::syscall(libc::SYS_landlock_restrict_self, ruleset, 0u32) } != 0 {
            return Err(format!(
                "Landlock could not be applied ({}), so the helper's files are not confined",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }

    /// Adds one path to the ruleset. A missing path the helper needs is an
    /// error; a missing system folder is skipped.
    fn allow(ruleset: libc::c_int, path: &Path, access: u64, needed: bool) -> Result<(), String> {
        let Ok(name) = CString::new(path.as_os_str().as_bytes()) else {
            return Err(format!("`{}` has a NUL in it", path.display()));
        };
        // Safety: a NUL-terminated path and plain flags.
        let fd = unsafe { libc::open(name.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
        if fd < 0 {
            return if needed {
                Err(format!(
                    "`{}` could not be opened ({}), so the helper's files are not confined",
                    path.display(),
                    std::io::Error::last_os_error()
                ))
            } else {
                Ok(())
            };
        }
        // A file takes file rights only, or the kernel refuses the rule.
        let is_dir = std::fs::metadata(path)
            .map(|meta| meta.is_dir())
            .unwrap_or(false);
        let attr = PathBeneathAttr {
            allowed_access: if is_dir { access } else { access & FS_FILE },
            parent_fd: fd,
        };
        // Safety: `attr` lives across the call; `fd` is ours.
        let added = unsafe {
            libc::syscall(
                libc::SYS_landlock_add_rule,
                ruleset,
                RULE_PATH_BENEATH,
                &attr as *const PathBeneathAttr,
                0u32,
            )
        };
        let error = std::io::Error::last_os_error();
        // Safety: ours, and the rule holds its own reference.
        unsafe { libc::close(fd) };
        if added != 0 {
            return Err(format!(
                "Landlock refused a rule for `{}` ({error}), so the helper's files are not confined",
                path.display()
            ));
        }
        Ok(())
    }

    // Classic BPF, from <linux/filter.h> and <linux/seccomp.h>.
    const BPF_LD_W_ABS: u16 = 0x20;
    const BPF_JMP_JEQ_K: u16 = 0x15;
    const BPF_JMP_JGE_K: u16 = 0x35;
    const BPF_RET_K: u16 = 0x06;
    const RET_ALLOW: u32 = 0x7fff_0000;
    const RET_ERRNO: u32 = 0x0005_0000;
    const RET_KILL_PROCESS: u32 = 0x8000_0000;
    /// `seccomp_data.nr` and `seccomp_data.arch`.
    const OFFSET_NR: u32 = 0;
    const OFFSET_ARCH: u32 = 4;

    #[cfg(target_arch = "x86_64")]
    const ARCH: Option<u32> = Some(0xc000_003e);
    #[cfg(target_arch = "aarch64")]
    const ARCH: Option<u32> = Some(0xc000_00b7);
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    const ARCH: Option<u32> = None;

    /// The syscalls `run` never makes, and an escaped guest would want.
    fn denied() -> Vec<libc::c_long> {
        vec![
            libc::SYS_socket,
            libc::SYS_ptrace,
            libc::SYS_process_vm_readv,
            libc::SYS_process_vm_writev,
            libc::SYS_execve,
            libc::SYS_execveat,
            libc::SYS_io_uring_setup,
            libc::SYS_io_uring_enter,
            libc::SYS_io_uring_register,
        ]
    }

    /// The filter: a foreign architecture's call (x86's `int 0x80`, x32) kills
    /// the process, a denied call answers `EPERM`, and everything else runs.
    pub(super) fn program() -> Option<Vec<libc::sock_filter>> {
        let arch = ARCH?;
        let op = |code, jt, jf, k| libc::sock_filter { code, jt, jf, k };
        let denied = denied();
        let mut filter = vec![
            op(BPF_LD_W_ABS, 0, 0, OFFSET_ARCH),
            op(BPF_JMP_JEQ_K, 1, 0, arch),
            op(BPF_RET_K, 0, 0, RET_KILL_PROCESS),
            op(BPF_LD_W_ABS, 0, 0, OFFSET_NR),
        ];
        if cfg!(target_arch = "x86_64") {
            // x32 calls share the architecture with bit 30 set.
            filter.push(op(BPF_JMP_JGE_K, 0, 1, 0x4000_0000));
            filter.push(op(BPF_RET_K, 0, 0, RET_KILL_PROCESS));
        }
        for (index, call) in denied.iter().enumerate() {
            // A match jumps over the rest of the checks and the allow.
            let skip = (denied.len() - index) as u8;
            filter.push(op(BPF_JMP_JEQ_K, skip, 0, *call as u32));
        }
        filter.push(op(BPF_RET_K, 0, 0, RET_ALLOW));
        filter.push(op(BPF_RET_K, 0, 0, RET_ERRNO | libc::EPERM as u32));
        Some(filter)
    }

    fn seccomp() -> Result<(), String> {
        let Some(mut filter) = program() else {
            return Err("there is no seccomp filter for this architecture, so the helper may still open sockets".to_owned());
        };
        let prog = libc::sock_fprog {
            len: filter.len() as u16,
            filter: filter.as_mut_ptr(),
        };
        const SET_MODE_FILTER: libc::c_uint = 1;
        const FLAG_TSYNC: libc::c_ulong = 1;
        // Safety: `prog` and the filter it points at live across the call.
        let result = unsafe {
            libc::syscall(
                libc::SYS_seccomp,
                SET_MODE_FILTER,
                FLAG_TSYNC,
                &prog as *const _,
            )
        };
        if result != 0 {
            return Err(format!(
                "seccomp could not be applied ({}), so the helper may still open sockets",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }
}

/// `yapfast-plugins sandbox-check STORE CACHE OUTSIDE`: applies the sandbox
/// as `run` would and reports, one `key=value` line each, whether the store
/// can be read, the cache written, `OUTSIDE` read, a socket opened, and a
/// program started. The tests run it in a process of its own, because a
/// sandbox can't be taken off the test runner again.
pub fn check(arguments: Vec<String>) -> i32 {
    let [store, cache, outside] = arguments.as_slice() else {
        eprintln!("usage: yapfast-plugins sandbox-check STORE CACHE OUTSIDE");
        return 2;
    };
    let paths = Paths {
        read: vec![PathBuf::from(store)],
        write: vec![PathBuf::from(cache)],
    };
    // Safety: `main` calls this before anything starts a thread.
    let report = unsafe { apply(&paths) };
    println!("applied={}", report.applied.join(","));
    for reason in &report.missing {
        println!("missing={reason}");
    }
    let read_store = std::fs::read_dir(store).is_ok();
    let write_cache = std::fs::write(Path::new(cache).join("probe"), b"ok").is_ok();
    let read_outside = std::fs::read(outside).is_ok();
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").is_ok();
    let exec = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg("true")
        .status()
        .is_ok();
    println!("read_store={read_store}");
    println!("write_cache={write_cache}");
    println!("read_outside={read_outside}");
    println!("socket={socket}");
    println!("exec={exec}");
    0
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    #[test]
    fn the_filter_ends_in_allow_then_deny() {
        let Some(filter) = super::linux::program() else {
            return;
        };
        let last = &filter[filter.len() - 1];
        let allow = &filter[filter.len() - 2];
        assert_eq!(
            last.k & 0xffff_0000,
            0x0005_0000,
            "the last instruction answers EPERM"
        );
        assert_eq!(allow.k, 0x7fff_0000, "the one before it allows");
        // Every denial jumps exactly to the EPERM answer.
        for (index, op) in filter.iter().enumerate() {
            if op.code == 0x15 && op.jt > 1 {
                assert_eq!(index + 1 + op.jt as usize, filter.len() - 1);
            }
        }
    }
}

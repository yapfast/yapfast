use super::{HashMap, OsString, PathBuf, Process, io};
use std::{fs, os::unix::ffi::OsStringExt, path::Path};

struct Clock {
    boot: u64,
    ticks: u64,
}

fn clock() -> io::Result<Clock> {
    let boot = fs::read_to_string("/proc/stat")?
        .lines()
        .find_map(|line| line.strip_prefix("btime ")?.parse().ok())
        .ok_or(io::ErrorKind::InvalidData)?;
    // SAFETY: sysconf takes a constant and has no pointer arguments.
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if ticks <= 0 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(Clock {
        boot,
        ticks: ticks as u64,
    })
}

fn stat(path: &Path) -> Option<(OsString, u64)> {
    let data = fs::read(path.join("stat")).ok()?;
    let open = data.iter().position(|&byte| byte == b'(')?;
    let close = data.iter().rposition(|&byte| byte == b')')?;
    let name = OsString::from_vec(data.get(open + 1..close)?.to_vec());
    // The fields after comm start at field 3 (state). Start time is field 22.
    let ticks = std::str::from_utf8(data.get(close + 1..)?)
        .ok()?
        .split_ascii_whitespace()
        .nth(19)?
        .parse()
        .ok()?;
    Some((name, ticks))
}

pub(super) fn scan() -> io::Result<HashMap<u32, Process>> {
    let clock = clock()?;
    // SAFETY: getuid has no arguments or preconditions.
    let uid = unsafe { libc::getuid() };
    let mut processes = HashMap::new();
    for entry in fs::read_dir("/proc")?.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse().ok())
        else {
            continue;
        };
        let path = entry.path();
        let Ok(status) = fs::read(path.join("status")) else {
            continue;
        };
        let owner = status.split(|&byte| byte == b'\n').find_map(|line| {
            std::str::from_utf8(line.strip_prefix(b"Uid:")?)
                .ok()?
                .split_ascii_whitespace()
                .next()?
                .parse::<u32>()
                .ok()
        });
        if owner != Some(uid) {
            continue;
        }
        let Some((name, ticks)) = stat(&path) else {
            continue;
        };
        // Read the executable each scan: exec can change it under the same PID.
        let exe = fs::read_link(path.join("exe")).ok();
        let started = clock.boot + ticks / clock.ticks;
        processes.insert(pid, Process { name, exe, started });
    }
    Ok(processes)
}

pub(super) fn arguments(pid: u32, _started: u64) -> Option<Vec<OsString>> {
    let data = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    if data.is_empty() {
        return Some(Vec::new());
    }
    let data = data.strip_suffix(&[0]).unwrap_or(&data);
    Some(
        data.split(|&byte| byte == 0)
            .map(|arg| OsString::from_vec(arg.to_vec()))
            .collect(),
    )
}

pub(super) fn cwd(pid: u32, _started: u64) -> Option<PathBuf> {
    fs::read_link(format!("/proc/{pid}/cwd")).ok()
}

use super::{HashMap, OsString, PathBuf, Process, io};
use std::{
    mem::{MaybeUninit, size_of, size_of_val},
    os::unix::ffi::OsStringExt,
    ptr::null_mut,
};

fn info(pid: libc::pid_t) -> Option<libc::proc_bsdinfo> {
    let mut info = MaybeUninit::<libc::proc_bsdinfo>::uninit();
    let size = size_of::<libc::proc_bsdinfo>() as i32;
    // SAFETY: info provides the output storage required by PROC_PIDTBSDINFO.
    unsafe {
        (libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        ) == size)
            .then(|| info.assume_init())
    }
}

pub(super) fn scan() -> io::Result<HashMap<u32, Process>> {
    // SAFETY: a null buffer asks libproc how many PIDs it could return.
    let estimate = unsafe { libc::proc_listallpids(null_mut(), 0) };
    if estimate <= 0 {
        return Err(io::Error::last_os_error());
    }
    let mut pids = vec![0; estimate as usize + 32];
    loop {
        // SAFETY: pids provides the stated number of writable bytes.
        let count = unsafe {
            libc::proc_listallpids(
                pids.as_mut_ptr().cast(),
                size_of_val(pids.as_slice()) as i32,
            )
        };
        if count <= 0 {
            return Err(io::Error::last_os_error());
        }
        if (count as usize) < pids.len() {
            pids.truncate(count as usize);
            break;
        }
        pids.resize(pids.len() * 2, 0);
    }
    // SAFETY: getuid has no arguments or preconditions.
    let uid = unsafe { libc::getuid() };
    let mut processes = HashMap::new();
    let mut path = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    for pid in pids {
        let Some(info) = info(pid).filter(|info| info.pbi_ruid == uid) else {
            continue;
        };
        // SAFETY: path is writable for the stated size.
        let len = unsafe { libc::proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) };
        let exe = (len > 0).then(|| {
            let bytes = &path[..len as usize];
            PathBuf::from(OsString::from_vec(
                bytes.strip_suffix(&[0]).unwrap_or(bytes).to_vec(),
            ))
        });
        let name = OsString::from_vec(
            info.pbi_comm
                .into_iter()
                .take_while(|&byte| byte != 0)
                .map(|byte| byte as u8)
                .collect(),
        );
        processes.insert(
            pid as u32,
            Process {
                name,
                exe,
                started: info.pbi_start_tvsec,
            },
        );
    }
    Ok(processes)
}

pub(super) fn arguments(pid: u32, started: u64) -> Option<Vec<OsString>> {
    if info(pid as i32)?.pbi_start_tvsec != started {
        return None;
    }
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as i32];
    let mut len = 0;
    // SAFETY: this read-only query writes the required length to len.
    if unsafe { libc::sysctl(mib.as_mut_ptr(), 3, null_mut(), &mut len, null_mut(), 0) } != 0 {
        return None;
    }
    let mut data = vec![0u8; len];
    // SAFETY: data can hold len bytes and the MIB has three integers.
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            data.as_mut_ptr().cast(),
            &mut len,
            null_mut(),
            0,
        )
    } != 0
    {
        return None;
    }
    let data = data.get(..len)?;
    let count = i32::from_ne_bytes(data.get(..4)?.try_into().ok()?);
    let count = usize::try_from(count).ok()?;
    // The executable path and its NUL padding precede argv. Stop at argc.
    let payload = data.get(4..)?;
    let payload = payload.get(payload.iter().position(|&byte| byte == 0)?..)?;
    let payload = payload.get(payload.iter().position(|&byte| byte != 0)?..)?;
    Some(
        payload
            .split(|&byte| byte == 0)
            .take(count)
            .map(|arg| OsString::from_vec(arg.to_vec()))
            .collect(),
    )
}

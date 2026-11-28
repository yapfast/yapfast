use std::{collections::HashMap, ffi::OsString, io, path::PathBuf};

pub(super) struct Process {
    pub name: OsString,
    pub exe: Option<PathBuf>,
    pub started: u64,
}

#[cfg(target_os = "linux")]
#[path = "processes/linux.rs"]
mod platform;
#[cfg(windows)]
#[path = "processes/windows.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "processes/macos.rs"]
mod platform;

pub(super) fn scan() -> io::Result<HashMap<u32, Process>> {
    #[cfg(any(target_os = "linux", windows, target_os = "macos"))]
    {
        platform::scan()
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    Err(io::ErrorKind::Unsupported.into())
}

pub(super) fn arguments(pid: u32, started: u64) -> Vec<OsString> {
    #[cfg(any(target_os = "linux", windows, target_os = "macos"))]
    {
        platform::arguments(pid, started).unwrap_or_default()
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        let _ = (pid, started);
        Vec::new()
    }
}

pub(super) fn cwd(pid: u32, started: u64) -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        platform::cwd(pid, started)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (pid, started);
        None
    }
}

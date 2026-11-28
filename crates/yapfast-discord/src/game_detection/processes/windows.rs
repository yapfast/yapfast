use super::{HashMap, OsString, PathBuf, Process, io};
use std::{
    mem::{size_of, zeroed},
    os::windows::ffi::OsStringExt,
    ptr::{null_mut, read_unaligned},
};
use windows_sys::{
    Wdk::System::Threading::{NtQueryInformationProcess, ProcessCommandLineInformation},
    Win32::{
        Foundation::{
            CloseHandle, ERROR_NO_MORE_FILES, FILETIME, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
            UNICODE_STRING,
        },
        Security::{EqualSid, GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                GetCurrentProcess, GetProcessTimes, OpenProcess, OpenProcessToken,
                PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
            },
        },
        UI::Shell::CommandLineToArgvW,
    },
};

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: this guard owns a valid kernel handle.
        unsafe { CloseHandle(self.0) };
    }
}

struct User([usize; 24]);

impl User {
    fn read(process: HANDLE) -> Option<Self> {
        let mut token = null_mut();
        // SAFETY: process is a live handle and token is writable.
        if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
            return None;
        }
        let token = Handle(token);
        let mut user = Self([0; 24]);
        let mut length = 0;
        // SAFETY: storage is aligned and large enough for TOKEN_USER and a maximum SID.
        let ok = unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                user.0.as_mut_ptr().cast(),
                size_of::<Self>() as u32,
                &mut length,
            )
        };
        if ok == 0 {
            return None;
        }
        // Store the SID offset so moving User does not leave a stale pointer.
        // SAFETY: the successful query initialized the TOKEN_USER header.
        let sid = unsafe { (*user.0.as_ptr().cast::<TOKEN_USER>()).User.Sid };
        user.0[0] = sid as usize - user.0.as_ptr() as usize;
        Some(user)
    }

    fn matches(&self, other: &Self) -> bool {
        // SAFETY: both offsets locate the SIDs written into the respective buffers.
        unsafe {
            EqualSid(
                self.0
                    .as_ptr()
                    .cast::<u8>()
                    .add(self.0[0])
                    .cast_mut()
                    .cast(),
                other
                    .0
                    .as_ptr()
                    .cast::<u8>()
                    .add(other.0[0])
                    .cast_mut()
                    .cast(),
            ) != 0
        }
    }
}

fn open(pid: u32) -> Option<Handle> {
    // SAFETY: OpenProcess takes a PID and flags and returns an owned handle on success.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    (!process.is_null()).then(|| Handle(process))
}

fn start_time(process: &Handle) -> Option<u64> {
    // SAFETY: FILETIME permits zero initialization and the outputs are writable.
    unsafe {
        let (mut created, mut exited, mut kernel, mut user) =
            (zeroed::<FILETIME>(), zeroed(), zeroed(), zeroed());
        if GetProcessTimes(process.0, &mut created, &mut exited, &mut kernel, &mut user) == 0 {
            return None;
        }
        let ticks = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
        (ticks / 10_000_000).checked_sub(11_644_473_600)
    }
}

pub(super) fn scan() -> io::Result<HashMap<u32, Process>> {
    // SAFETY: GetCurrentProcess returns a valid pseudo-handle.
    let user = User::read(unsafe { GetCurrentProcess() }).ok_or_else(io::Error::last_os_error)?;
    // SAFETY: the process snapshot takes flags and no pointers.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let snapshot = Handle(snapshot);
    // SAFETY: PROCESSENTRY32W permits zero initialization. The API requires dwSize.
    let mut entry: PROCESSENTRY32W = unsafe { zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    // SAFETY: snapshot and the initialized output are valid.
    if unsafe { Process32FirstW(snapshot.0, &mut entry) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut processes = HashMap::new();
    let mut image = vec![0u16; 32_768];
    loop {
        let pid = entry.th32ProcessID;
        if let Some(process) = open(pid)
            && User::read(process.0).is_some_and(|owner| user.matches(&owner))
            && let Some(started) = start_time(&process)
        {
            let mut len = image.len() as u32;
            // SAFETY: the handle is live and len describes the writable image buffer.
            let exe =
                (unsafe { QueryFullProcessImageNameW(process.0, 0, image.as_mut_ptr(), &mut len) }
                    != 0)
                    .then(|| PathBuf::from(OsString::from_wide(&image[..len as usize])));
            let name = entry
                .szExeFile
                .split(|&unit| unit == 0)
                .next()
                .unwrap_or_default();
            processes.insert(
                pid,
                Process {
                    name: OsString::from_wide(name),
                    exe,
                    started,
                },
            );
        }
        // SAFETY: snapshot and the initialized output are valid.
        if unsafe { Process32NextW(snapshot.0, &mut entry) } == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_NO_MORE_FILES as i32) {
                return Err(error);
            }
            break;
        }
    }
    Ok(processes)
}

pub(super) fn arguments(pid: u32, started: u64) -> Option<Vec<OsString>> {
    let process = open(pid)?;
    if start_time(&process)? != started {
        return None;
    }
    let mut size = 0;
    // SAFETY: a null output and zero capacity request the required size.
    unsafe {
        NtQueryInformationProcess(
            process.0,
            ProcessCommandLineInformation,
            null_mut(),
            0,
            &mut size,
        )
    };
    if !(size_of::<UNICODE_STRING>() as u32..=1_048_576).contains(&size) {
        return None;
    }
    let mut buffer = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
    let base = buffer.as_mut_ptr().cast::<u8>();
    // SAFETY: the live handle and output allocation accommodate the advertised byte count.
    let status = unsafe {
        NtQueryInformationProcess(
            process.0,
            ProcessCommandLineInformation,
            base.cast(),
            size,
            &mut size,
        )
    };
    if status < 0 {
        return None;
    }
    // SAFETY: the successful query wrote a UNICODE_STRING at the start of the buffer.
    let command = unsafe { read_unaligned(base.cast::<UNICODE_STRING>()) };
    let offset = (command.Buffer as usize).checked_sub(base as usize)?;
    let length = usize::from(command.Length);
    if length == 0
        || !offset.is_multiple_of(2)
        || !length.is_multiple_of(2)
        || offset.checked_add(length)? > buffer.len() * size_of::<usize>()
    {
        return None;
    }
    // SAFETY: the UTF-16 range was checked against the allocation.
    let mut line =
        unsafe { std::slice::from_raw_parts(base.add(offset).cast::<u16>(), length / 2) }.to_vec();
    line.push(0);
    let mut count = 0;
    // SAFETY: line is NUL-terminated and count is writable.
    let argv = unsafe { CommandLineToArgvW(line.as_ptr(), &mut count) };
    if argv.is_null() {
        return None;
    }
    // SAFETY: the API returns count pointers to NUL-terminated strings in one allocation.
    let args = unsafe {
        let args = std::slice::from_raw_parts(argv, count as usize)
            .iter()
            .map(|&arg| {
                let mut len = 0;
                while *arg.add(len) != 0 {
                    len += 1;
                }
                OsString::from_wide(std::slice::from_raw_parts(arg, len))
            })
            .collect();
        LocalFree(argv.cast());
        args
    };
    Some(args)
}

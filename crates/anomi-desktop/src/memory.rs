//! Giving memory back to the system once what used it is gone: the window
//! closed to the tray, a picker of many pictures closed.

/// Gives the allocator's free memory back. glibc keeps freed heap for reuse
/// until trimmed. Windows keeps what a burst of allocations freed, ready for
/// the next burst, in heaps the low-fragmentation heap makes for those
/// sizes: `HeapCompact` joins freed blocks but returns none of the heaps;
/// `HeapOptimizeResources` gives back what they hold.
pub fn trim() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        unsafe extern "C" {
            fn malloc_trim(pad: usize) -> std::ffi::c_int;
        }
        // SAFETY: malloc_trim only releases free memory held by the allocator.
        unsafe { malloc_trim(0) };
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Memory::{HeapOptimizeResources, HeapSetInformation};
        // HEAP_OPTIMIZE_RESOURCES_INFORMATION, which windows-sys doesn't name.
        #[repr(C)]
        struct OptimizeResources {
            version: u32,
            flags: u32,
        }
        // HEAP_OPTIMIZE_RESOURCES_CURRENT_VERSION.
        const CURRENT_VERSION: u32 = 1;
        let info = OptimizeResources {
            version: CURRENT_VERSION,
            flags: 0,
        };
        // SAFETY: the information is the one HeapOptimizeResources takes, a
        // null heap handle means every heap of the process, and optimizing
        // them moves nothing in use.
        unsafe {
            HeapSetInformation(
                std::ptr::null_mut(),
                HeapOptimizeResources,
                std::ptr::from_ref(&info).cast(),
                std::mem::size_of::<OptimizeResources>(),
            );
        }
    }
}

/// Gives back memory freed with the window: [`trim`], and on Windows the
/// heap compacted and the working set emptied, as Task Manager shows the
/// private working set, which pages the window used would otherwise stay in.
pub fn release() {
    trim();
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::{
            Memory::{GetProcessHeap, HeapCompact},
            Threading::{GetCurrentProcess, SetProcessWorkingSetSize},
        };
        // SAFETY: both act on this process's own heap and working set; -1 for
        // both sizes asks Windows to take out every page it can.
        unsafe {
            HeapCompact(GetProcessHeap(), 0);
            SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
        }
    }
    tracing::info!(target: crate::LOG, "memory released");
}

/// Prints the allocator's per-arena totals to standard error: memory held
/// from the system against memory in use (`scripts/ui-session do 'window
/// stats'`).
pub fn stats() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        unsafe extern "C" {
            fn malloc_stats();
        }
        // SAFETY: malloc_stats only reads allocator state and writes to stderr.
        unsafe { malloc_stats() };
    }
}

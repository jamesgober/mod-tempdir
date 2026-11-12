//! Small platform helpers: file identity and process liveness.
//!
//! Everything here uses `std` plus a few functions from the C library
//! (Unix) or `kernel32` (Windows) that `std` already links. No extra
//! dependency is added.

use std::fs::File;
use std::io;
use std::path::Path;

/// Identity of an open file: `(device, inode)` on Unix,
/// `(volume serial number, file index)` on Windows.
pub(crate) type FileId = (u64, u64);

/// Return the identity of the file behind `file`.
///
/// On platforms where no identity is available, returns `None`.
pub(crate) fn file_id(file: &File) -> io::Result<Option<FileId>> {
    imp::file_id(file)
}

/// Return the identity of the entry at `path` without following a
/// final symbolic link or reparse point, so a link planted at `path`
/// never reports the identity of its target.
pub(crate) fn path_id_no_follow(path: &Path) -> io::Result<Option<FileId>> {
    imp::path_id_no_follow(path)
}

/// Return `true` unless process `pid` is known to have exited.
///
/// The check errs on the side of "alive": any answer the platform
/// cannot give clearly counts as alive, so a sweep never removes the
/// entry of a process that may still be running.
pub(crate) fn pid_alive(pid: u32) -> bool {
    if pid == std::process::id() {
        return true;
    }
    imp::pid_alive(pid)
}

#[cfg(unix)]
mod imp {
    use super::FileId;
    use std::fs::File;
    use std::io;
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;

    pub(super) fn file_id(file: &File) -> io::Result<Option<FileId>> {
        let m = file.metadata()?;
        Ok(Some((m.dev(), m.ino())))
    }

    pub(super) fn path_id_no_follow(path: &Path) -> io::Result<Option<FileId>> {
        let m = std::fs::symlink_metadata(path)?;
        Ok(Some((m.dev(), m.ino())))
    }

    #[cfg(target_os = "linux")]
    pub(super) fn pid_alive(pid: u32) -> bool {
        if pid == 0 {
            return false;
        }
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }

    #[cfg(not(target_os = "linux"))]
    pub(super) fn pid_alive(pid: u32) -> bool {
        extern "C" {
            fn kill(pid: i32, sig: i32) -> i32;
        }
        const EPERM: i32 = 1;
        const ESRCH: i32 = 3;

        // PID 0 and negative values address process groups, not one
        // process. Neither can be the owner of an entry.
        let pid = match i32::try_from(pid) {
            Ok(p) if p > 0 => p,
            _ => return false,
        };
        // SAFETY: `kill` with signal 0 sends no signal; it only checks
        // whether `pid` exists and may be signalled. `pid` is a single
        // positive process ID, never a group selector.
        let rc = unsafe { kill(pid, 0) };
        if rc == 0 {
            return true;
        }
        match io::Error::last_os_error().raw_os_error() {
            Some(ESRCH) => false,
            Some(EPERM) => true,
            _ => true,
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::FileId;
    use std::ffi::c_void;
    use std::fs::File;
    use std::io;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;

    type Handle = *mut c_void;

    #[repr(C)]
    #[derive(Default)]
    #[allow(dead_code)] // filled in by the OS; only some fields are read
    struct ByHandleFileInformation {
        file_attributes: u32,
        creation_time: [u32; 2],
        last_access_time: [u32; 2],
        last_write_time: [u32; 2],
        volume_serial_number: u32,
        file_size_high: u32,
        file_size_low: u32,
        number_of_links: u32,
        file_index_high: u32,
        file_index_low: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetFileInformationByHandle(file: Handle, info: *mut ByHandleFileInformation) -> i32;
        fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> Handle;
        fn GetExitCodeProcess(process: Handle, exit_code: *mut u32) -> i32;
        fn CloseHandle(object: Handle) -> i32;
    }

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_SHARE_ALL: u32 = 0x1 | 0x2 | 0x4;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const ERROR_INVALID_PARAMETER: i32 = 87;
    const STILL_ACTIVE: u32 = 259;

    pub(super) fn file_id(file: &File) -> io::Result<Option<FileId>> {
        let mut info = ByHandleFileInformation::default();
        // SAFETY: the handle is owned by `file` and stays open for the
        // duration of the call; `info` is a valid, writable
        // `BY_HANDLE_FILE_INFORMATION` with the documented layout.
        let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let index = (u64::from(info.file_index_high) << 32) | u64::from(info.file_index_low);
        Ok(Some((u64::from(info.volume_serial_number), index)))
    }

    pub(super) fn path_id_no_follow(path: &Path) -> io::Result<Option<FileId>> {
        // Attribute-only open (no read or write access), every share
        // mode, and `FILE_FLAG_OPEN_REPARSE_POINT` so a symbolic link
        // or junction at `path` is opened itself, not its target.
        let handle = std::fs::OpenOptions::new()
            .access_mode(0)
            .share_mode(FILE_SHARE_ALL)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        file_id(&handle)
    }

    pub(super) fn pid_alive(pid: u32) -> bool {
        // SAFETY: `OpenProcess` takes plain integer arguments and
        // returns either NULL or a process handle we own.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            // `ERROR_INVALID_PARAMETER` means no process has this ID.
            // Anything else (typically access denied for another
            // user's process) means it exists.
            return io::Error::last_os_error().raw_os_error() != Some(ERROR_INVALID_PARAMETER);
        }
        let mut code: u32 = 0;
        // SAFETY: `process` is a valid handle opened above with query
        // access; `code` is a valid `u32` out-parameter.
        let ok = unsafe { GetExitCodeProcess(process, &mut code) };
        // SAFETY: `process` was returned by `OpenProcess`, is not used
        // after this call, and is closed exactly once.
        unsafe {
            CloseHandle(process);
        }
        // A failed query counts as alive. A process that exited with
        // code 259 also reads as alive, which only makes the sweep
        // more conservative.
        ok == 0 || code == STILL_ACTIVE
    }
}

#[cfg(not(any(unix, windows)))]
mod imp {
    use super::FileId;
    use std::fs::File;
    use std::io;
    use std::path::Path;

    pub(super) fn file_id(_file: &File) -> io::Result<Option<FileId>> {
        Ok(None)
    }

    pub(super) fn path_id_no_follow(_path: &Path) -> io::Result<Option<FileId>> {
        Ok(None)
    }

    pub(super) fn pid_alive(_pid: u32) -> bool {
        // No process introspection without platform calls; count every
        // process as alive so nothing is removed on liveness grounds.
        true
    }
}

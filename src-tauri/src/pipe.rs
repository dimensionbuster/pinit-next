//! Named pipe transport for the live-video bridge. No network port is opened.
//!
//! * The name is fixed per build identity (`\\.\pipe\<identifier>.video`), so there is no
//!   discovery file that could go stale.
//! * The server refuses to start if another process already owns the name
//!   (`FILE_FLAG_FIRST_PIPE_INSTANCE`), rejects remote clients, and only the owner of the
//!   pipe (the current user) may connect (DACL `D:P(A;;GA;;;OW)`).
//! * The client side is plain `std::fs::File`.
//!
//! The pipe handles are synchronous: one thread must not block in a read while another writes
//! the same handle. The app only reads; the host only writes (it detects a closed stream with
//! [`is_broken`]).

use std::fs::{File, OpenOptions};
use std::io;

pub fn pipe_path() -> String {
    format!(r"\\.\pipe\{}.video", crate::paths::IDENTIFIER)
}

/// Client: open the app's pipe. `NotFound` means the app is not running.
pub fn connect() -> io::Result<File> {
    connect_to(&pipe_path())
}

fn connect_to(path: &str) -> io::Result<File> {
    OpenOptions::new().read(true).write(true).open(path)
}

#[cfg(windows)]
pub use imp::{is_broken, Canceller, PipeServer};

#[cfg(windows)]
mod imp {
    use std::fs::File;
    use std::io;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Foundation::{
        GetLastError, LocalFree, ERROR_PIPE_CONNECTED, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
    use windows_sys::Win32::System::IO::CancelIoEx;
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, PeekNamedPipe, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
        PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };

    /// Only the pipe's owner (the user running PinIt) gets access.
    const SDDL: &str = "D:P(A;;GA;;;OW)";
    const BUFFER: u32 = 64 * 1024;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Creates one pipe instance. `first` demands that the name is not in use yet.
    fn create(name: &[u16], first: bool) -> io::Result<isize> {
        let sddl = wide(SDDL);
        let mut descriptor = null_mut();
        // SAFETY: valid NUL-terminated UTF-16 string and out-pointer; the result is freed below.
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut descriptor, null_mut())
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let open_mode = PIPE_ACCESS_DUPLEX | if first { FILE_FLAG_FIRST_PIPE_INSTANCE } else { 0 };
        // SAFETY: `name` is NUL-terminated; `attributes` outlives the call.
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                open_mode,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                BUFFER,
                BUFFER,
                0,
                &attributes,
            )
        };
        let error = io::Error::last_os_error();
        // SAFETY: allocated by ConvertStringSecurityDescriptor...; not used after this.
        unsafe { LocalFree(descriptor) };
        if handle == INVALID_HANDLE_VALUE {
            return Err(error);
        }
        Ok(handle as isize)
    }

    /// Accepts clients one after another; a fresh instance is always waiting.
    pub struct PipeServer {
        name: Vec<u16>,
        /// Instance waiting for the next client (0 = could not be created).
        pending: isize,
    }

    impl PipeServer {
        /// Fails if another process already owns the pipe name.
        pub fn bind() -> io::Result<Self> {
            Self::bind_to(&super::pipe_path())
        }

        pub(super) fn bind_to(path: &str) -> io::Result<Self> {
            let name = wide(path);
            let pending = create(&name, true)?;
            Ok(Self { name, pending })
        }

        /// Blocks until a client connects and returns the connected end.
        pub fn accept(&mut self) -> io::Result<File> {
            if self.pending == 0 {
                self.pending = create(&self.name, false)?;
            }
            let handle = self.pending as HANDLE;
            // SAFETY: `handle` is a pipe instance we own; no OVERLAPPED is used.
            let connected = unsafe { ConnectNamedPipe(handle, null_mut()) } != 0
                || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
            if !connected {
                return Err(io::Error::last_os_error());
            }
            self.pending = create(&self.name, false).unwrap_or(0);
            // SAFETY: we own the handle and hand that ownership to the File.
            Ok(unsafe { File::from_raw_handle(handle as RawHandle) })
        }
    }

    /// Cancels a blocking read on a pipe from another thread. The `File` must outlive its use.
    #[derive(Clone, Copy)]
    pub struct Canceller(isize);

    impl Canceller {
        pub fn new(file: &File) -> Self {
            Self(file.as_raw_handle() as isize)
        }

        pub fn cancel(&self) {
            // SAFETY: the owner keeps the handle open while the canceller can be called.
            unsafe { CancelIoEx(self.0 as HANDLE, null()) };
        }
    }

    /// True once the other end of the pipe is gone. Never blocks and takes no data.
    pub fn is_broken(file: &File) -> bool {
        // SAFETY: all optional out-pointers are null.
        unsafe { PeekNamedPipe(file.as_raw_handle() as HANDLE, null_mut(), 0, null_mut(), null_mut(), null_mut()) == 0 }
    }
}

#[cfg(not(windows))]
pub use stub::{is_broken, Canceller, PipeServer};

#[cfg(not(windows))]
mod stub {
    use std::fs::File;
    use std::io;

    pub struct PipeServer;
    impl PipeServer {
        pub fn bind() -> io::Result<Self> {
            Err(io::Error::new(io::ErrorKind::Unsupported, "named pipes need Windows"))
        }
        pub fn accept(&mut self) -> io::Result<File> {
            Err(io::Error::new(io::ErrorKind::Unsupported, "named pipes need Windows"))
        }
    }

    #[derive(Clone, Copy)]
    pub struct Canceller;
    impl Canceller {
        pub fn new(_: &File) -> Self {
            Self
        }
        pub fn cancel(&self) {}
    }

    pub fn is_broken(_: &File) -> bool {
        true
    }
}

#[cfg(all(test, windows))]
mod tests {
    use std::io::{BufRead, BufReader, Write};

    use super::*;

    #[test]
    fn client_reaches_server_and_second_server_is_refused() {
        // not the real name: a running dev build owns that one
        let path = format!(r"\\.\pipe\pinit-test-{}", std::process::id());
        let mut server = PipeServer::bind_to(&path).expect("first instance");
        assert!(PipeServer::bind_to(&path).is_err(), "name must not be shareable");

        let t = std::thread::spawn(move || {
            let file = server.accept().expect("accept");
            let mut line = String::new();
            BufReader::new(&file).read_line(&mut line).unwrap();
            line
        });
        let mut client = connect_to(&path).expect("connect");
        client.write_all(b"hello\n").unwrap();
        assert_eq!(t.join().unwrap(), "hello\n");
        // the server end was dropped: the client can tell
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(is_broken(&client));
    }
}

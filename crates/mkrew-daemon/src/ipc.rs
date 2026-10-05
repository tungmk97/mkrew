//! The local socket endpoint (SPEC §7): a named pipe on Windows, a Unix socket elsewhere.

use std::io;

use interprocess::local_socket::{ListenerOptions, Name, tokio::Listener};

/// Turns a [`Paths::socket`](mkrew_core::Paths) string into an `interprocess` name.
/// Clients use the same function, so both sides always agree.
pub fn name(socket: &str) -> io::Result<Name<'_>> {
    #[cfg(windows)]
    {
        use interprocess::local_socket::{GenericNamespaced, ToNsName};
        // `GenericNamespaced` adds the `\\.\pipe\` prefix itself.
        socket
            .strip_prefix(r"\\.\pipe\")
            .unwrap_or(socket)
            .to_ns_name::<GenericNamespaced>()
    }
    #[cfg(not(windows))]
    {
        use interprocess::local_socket::{GenericFilePath, ToFsName};
        socket.to_fs_name::<GenericFilePath>()
    }
}

/// Creates the listener, reachable by the current user only (SPEC §17).
///
/// Call only while holding the instance lock: on Unix a leftover socket file from a crashed
/// daemon is overwritten. On Windows a pipe that is still served by another process fails
/// with `PermissionDenied` (first-instance flag).
pub fn listen(socket: &str) -> io::Result<Listener> {
    let options = ListenerOptions::new().name(name(socket)?);
    #[cfg(windows)]
    let options = {
        use interprocess::os::windows::local_socket::ListenerOptionsExt;
        use interprocess::os::windows::security_descriptor::SecurityDescriptor;
        // Protected DACL: full access for LocalSystem and the pipe's owner (the user running
        // the daemon) only. The default DACL would let Everyone open it read-only.
        let sddl = widestring::U16CString::from_str("D:P(A;;GA;;;SY)(A;;GA;;;OW)")
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        options.security_descriptor(SecurityDescriptor::deserialize(&sddl)?)
    };
    #[cfg(unix)]
    let options = options.try_overwrite(true);
    // Setting the socket mode before bind only works on some Unixes (macOS rejects it);
    // elsewhere access relies on the 0700 runtime dir or the user's data dir.
    #[cfg(target_os = "linux")]
    let options = {
        use interprocess::os::unix::local_socket::ListenerOptionsExt;
        options.mode(0o600)
    };
    options.create_tokio()
}

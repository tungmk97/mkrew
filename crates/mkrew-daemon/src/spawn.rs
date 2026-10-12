//! Client-side helpers to start the daemon detached (SPEC §8), shared by `mk` and the UI.

use std::path::PathBuf;
use std::process::Command;

use anyhow::Context;

/// `mkrew-daemon` installed next to the running executable (the release layout and
/// `target/<profile>/`), else whatever `PATH` finds.
pub fn daemon_exe() -> anyhow::Result<PathBuf> {
    sibling_exe("mkrew-daemon")
}

/// `base` plus the platform's executable suffix, next to the running executable when it is a
/// file there, else the bare name for `PATH` to find. The daemon also finds `mk` this way.
pub fn sibling_exe(base: &str) -> anyhow::Result<PathBuf> {
    let name = format!("{base}{}", std::env::consts::EXE_SUFFIX);
    let exe = std::env::current_exe().context("locate the running executable")?;
    let sibling = exe.with_file_name(&name);
    Ok(if sibling.is_file() {
        sibling
    } else {
        PathBuf::from(name)
    })
}

/// No console window and no controlling terminal: the daemon outlives the shell (SPEC §8).
/// The UI also uses it for the short-lived `zed` CLI.
pub fn detach(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        disinherit_stdio();
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // New process group: Ctrl+C in the shell that ran the client does not reach the daemon.
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Own process group: terminal job-control signals (Ctrl+C, hangup) skip the daemon.
        command.process_group(0);
    }
}

/// `CreateProcess` passes EVERY inheritable handle to the child, not just its stdio. The
/// client's own stdio are inheritable when it runs with redirected output (`mk daemon start |
/// cat`, a test capturing output), and a daemon holding the write end of that pipe keeps the
/// reader waiting until the daemon exits. Clients only spawn detached children (the daemon,
/// `zed`) with explicit stdio, so clear the flag on all three.
#[cfg(windows)]
#[allow(unsafe_code)]
fn disinherit_stdio() {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};

    let handles = [
        std::io::stdin().as_raw_handle(),
        std::io::stdout().as_raw_handle(),
        std::io::stderr().as_raw_handle(),
    ];
    for handle in handles {
        // SAFETY: the handles are the process's live std handles (or null when absent, which
        // the call rejects harmlessly); only the inherit flag changes. Failure leaves the old
        // behavior.
        unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sibling_exe_prefers_the_file_next_to_the_running_executable() {
        let exe = std::env::current_exe().unwrap();
        let stem = exe.file_stem().unwrap().to_str().unwrap();
        assert_eq!(sibling_exe(stem).unwrap(), exe);

        let missing = format!("no-such-program{}", std::env::consts::EXE_SUFFIX);
        assert_eq!(
            sibling_exe("no-such-program").unwrap(),
            PathBuf::from(missing)
        );
    }
}

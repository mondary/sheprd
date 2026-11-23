//! Switching between the app's own window and Moonframe.
//!
//! A plain launch opens Moonframe in a build with it; `--classic` asks for the
//! app's own window. The other kind is therefore one relaunch away, with or
//! without that argument, and this process quits once the new one is on its way.

use std::ffi::OsString;

/// The arguments a relaunch needs to open the window kind this process is
/// not: `--classic` from Moonframe (a plain launch), none from the classic
/// window, whose plain launch is Moonframe. `None` when the build has no
/// Moonframe window to switch to.
pub(super) fn relaunch_args(current: &[OsString], moonframe: bool) -> Option<Vec<OsString>> {
    moonframe.then(|| {
        if current.is_empty() {
            vec![OsString::from("--classic")]
        } else {
            Vec::new()
        }
    })
}

/// Starts this executable again with `args`, once this process has quit.
#[cfg(unix)]
pub(super) fn relaunch(args: &[OsString]) {
    use std::process::Stdio;
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut command = sheprd_client::new_command("sh");
    command
        .arg("-c")
        .arg("sleep 1; exec \"$0\" \"$@\"")
        .arg(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Err(error) = command.spawn() {
        tracing::warn!(%error, "could not relaunch Sheprd in its other window");
    }
}

#[cfg(windows)]
pub(super) fn relaunch(args: &[OsString]) {
    use std::process::Stdio;
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut command = sheprd_client::new_command(&exe);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Err(error) = command.spawn() {
        tracing::warn!(%error, "could not relaunch Sheprd in its other window");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_launch_switches_to_the_classic_window() {
        // A plain launch opens Moonframe in a build with it, so its other
        // window is the classic one.
        assert_eq!(
            relaunch_args(&[], true),
            Some(vec![OsString::from("--classic")])
        );
    }

    #[test]
    fn a_classic_launch_switches_back_to_moonframe() {
        // Any argument means the app's own window was launched, so the plain
        // launch is what asks for Moonframe.
        assert_eq!(
            relaunch_args(&[OsString::from("--classic")], true),
            Some(Vec::new())
        );
        assert_eq!(
            relaunch_args(
                &[OsString::from("--session"), OsString::from("default")],
                true
            ),
            Some(Vec::new())
        );
    }

    #[test]
    fn a_build_without_moonframe_offers_no_switch() {
        assert_eq!(relaunch_args(&[], false), None);
        assert_eq!(relaunch_args(&[OsString::from("--classic")], false), None);
    }
}

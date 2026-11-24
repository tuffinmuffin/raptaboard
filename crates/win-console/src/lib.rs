//! The one unsafe call in raptaboard, kept apart so every other crate can
//! forbid unsafe code (the workspace's `unsafe_code = "forbid"`).
//!
//! A Windows release build of raptaboard is a window program (no console
//! window opens beside the dashboard), so what it prints goes nowhere.
//! `raptaboard packs …`, `--help` and `--version` print, so they first join
//! the console (terminal) they were started from: [`attach_parent`].

/// Join the console of the program that started this one (cmd, PowerShell
/// or Windows Terminal), so printing shows there. Whether it joined one:
/// `false` when started without one (from the Start menu or Explorer), or
/// already having one (a debug build), and always off Windows.
pub fn attach_parent() -> bool {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            // BOOL AttachConsole(DWORD dwProcessId);
            fn AttachConsole(process: u32) -> i32;
        }
        /// AttachConsole's "the parent process" (`(DWORD)-1`).
        const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
        // SAFETY: AttachConsole takes a process id by value and touches no
        // memory of ours; given ATTACH_PARENT_PROCESS it either joins the
        // parent's console or fails, returning 0.
        unsafe { AttachConsole(ATTACH_PARENT_PROCESS) != 0 }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    #[cfg(not(windows))]
    fn off_windows_there_is_nothing_to_join() {
        assert!(!attach_parent());
    }

    #[test]
    #[cfg(windows)]
    fn on_windows_it_returns_without_harm() {
        // A test already has a console (or none to join): either answer is
        // fine, it just mustn't crash.
        let _ = attach_parent();
    }
}

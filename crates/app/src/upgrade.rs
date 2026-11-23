//! One-time tasks for the first start of a new raptaboard version (and the
//! very first start): raptaboard remembers the version that last started
//! (`last_version` in `~/.raptaboard/content/state.json`, see
//! [`crate::content::State`]) and runs [`run`] when it's different.
//!
//! Add a task here when a new version needs to do something once, such as
//! moving a file or telling the user about a change; check `from` to do it
//! only when coming from an older version. Tasks must be safe to run again
//! (the state might not save) and must never need the network: anything
//! that downloads is asked for here and done later, as the content offer is.

use crate::content::{self, State};

/// What the first start of a new version does, given the version that
/// started before (`None` on the very first start, or from a version
/// older than 0.3.0, which didn't record it). Updates `state`; the caller
/// saves it.
///
/// ```
/// use raptaboard::content::State;
/// use raptaboard::upgrade::run;
/// let mut state = State::default();
/// run(None, &mut state);
/// assert!(state.offer_pending);
/// ```
pub fn run(from: Option<&str>, state: &mut State) {
    // Every new version offers the content packs not installed again (the
    // ones offered before unticked), so new packs, and packs that a new
    // version moved out of the app (the IT Crowd clips in 0.3.0), are seen.
    let _ = from;
    state.offer_pending = true;
}

/// Whether this start is the first of this version: run [`run`] if so,
/// and record the version. Returns the version that started before.
///
/// ```
/// use raptaboard::content::State;
/// use raptaboard::upgrade::start;
/// let mut state = State::default();
/// assert_eq!(start(&mut state), Some(None)); // the very first start
/// assert_eq!(start(&mut state), None); // the same version again
/// ```
pub fn start(state: &mut State) -> Option<Option<String>> {
    let now = content::app_version();
    if state.last_version.as_deref() == Some(now) {
        return None;
    }
    let from = state.last_version.replace(now.to_owned());
    run(from.as_deref(), state);
    Some(from)
}

//! GUI preference patches persisted off the UI thread, without a startup read.
use crate::logging::Logger;
use drift_core::{error::Error, gui_preferences::GuiPreferenceChange, store::Store};
use std::{
    io,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread::JoinHandle,
    time::{Duration, Instant},
};
use tokio::sync::watch;

const DEBOUNCE: Duration = Duration::from_millis(150);
const MAX_WAIT: Duration = Duration::from_secs(1);
const CLOSED_WARNING: &str =
    "GUI preference save could not be confirmed: the preference writer is closed.";
const WORKER_WARNING: &str =
    "GUI preference save could not be confirmed: the preference writer stopped unexpectedly.";

/// Cloneable UI handle. Only `finish` waits for persistence; call it after the
/// GUI loop returns. Dropping the final handle requests a drain but never joins.
#[derive(Clone)]
pub struct PreferencesWriter(Arc<Inner>);

struct Inner {
    shared: Arc<Shared>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    failure: watch::Sender<Option<String>>,
}

#[derive(Default)]
struct State {
    pending: Pending,
    closed: bool,
    unsaved: [Option<&'static str>; 4],
    rejected: bool,
    worker_failed: bool,
}

/// Each field occupies one slot, including an explicitly cleared pane ratio.
#[derive(Default)]
struct Pending {
    changes: [Option<GuiPreferenceChange>; 4],
    first: Option<Instant>,
    latest: Option<Instant>,
}

impl Pending {
    fn push(&mut self, change: GuiPreferenceChange) {
        let slot = match change {
            GuiPreferenceChange::Theme(_) => 0,
            GuiPreferenceChange::Window(_) => 1,
            GuiPreferenceChange::BrowserSplit(_) => 2,
            GuiPreferenceChange::ComparisonSplit(_) => 3,
        };
        let now = Instant::now();
        self.changes[slot] = Some(change);
        self.first.get_or_insert(now);
        self.latest = Some(now);
    }

    fn deadline(&self) -> Option<Instant> {
        Some((self.latest? + DEBOUNCE).min(self.first? + MAX_WAIT))
    }
}

impl State {
    fn warning(&self) -> Option<&'static str> {
        if self.rejected {
            Some(CLOSED_WARNING)
        } else if self.worker_failed {
            Some(WORKER_WARNING)
        } else {
            self.unsaved.iter().flatten().copied().next()
        }
    }
}

impl Shared {
    fn lock_state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| {
            let mut state = poisoned.into_inner();
            state.closed = true;
            state.worker_failed = true;
            self.publish(&state);
            state
        })
    }

    fn publish(&self, state: &State) {
        self.failure
            .send_replace(state.warning().map(str::to_owned));
    }

    fn close(&self) {
        self.lock_state().closed = true;
        self.wake.notify_one();
    }
}

impl PreferencesWriter {
    /// Spawn a worker only. Startup preferences must be loaded by the caller
    /// before entering the GUI; opening an idle writer never touches the store.
    pub fn open(store: Store, logger: Logger) -> io::Result<Self> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            wake: Condvar::new(),
            failure: watch::channel(None).0,
        });
        let background = shared.clone();
        let worker = std::thread::Builder::new()
            .name("drift-gui-preferences".into())
            .spawn(move || run(background, store, logger))?;
        Ok(Self(Arc::new(Inner {
            shared,
            worker: Mutex::new(Some(worker)),
        })))
    }

    /// Replace the latest value for this field. No filesystem work or waiting
    /// for the worker occurs here; rejected requests are exposed by `failures`.
    pub fn queue(&self, change: GuiPreferenceChange) {
        let shared = &self.0.shared;
        {
            let mut state = shared.lock_state();
            if state.closed {
                state.rejected = true;
                shared.publish(&state);
                return;
            }
            state.pending.push(change);
        }
        shared.wake.notify_one();
    }

    /// Retains a safe warning even when the GUI subscribes after a failure.
    /// A failed patch is discarded; only explicitly saving each failed field
    /// clears its warning. No raw configuration, paths or error text is exposed.
    pub fn failures(&self) -> watch::Receiver<Option<String>> {
        self.0.shared.failure.subscribe()
    }

    /// Atomically stop accepting changes, drain the latest patch and join.
    /// Repeated/concurrent calls retain the final result. Never call in Update
    /// or Drop: this is the synchronous shutdown boundary after the GUI exits.
    pub fn finish(&self) -> io::Result<()> {
        let shared = &self.0.shared;
        shared.close();
        // Serialize joiners separately: queue remains independent of this wait.
        let mut worker = self
            .0
            .worker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(worker) = worker.take()
            && worker.join().is_err()
        {
            let mut state = shared.lock_state();
            state.worker_failed = true;
            shared.publish(&state);
        }
        let state = shared.lock_state();
        match state.warning() {
            Some(warning) => Err(io::Error::other(warning)),
            None => Ok(()),
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // The worker owns Shared, not Inner, so the last UI handle can close it.
        self.shared.close();
    }
}

fn run(shared: Arc<Shared>, store: Store, logger: Logger) {
    loop {
        let changes = {
            let mut state = shared.lock_state();
            loop {
                if let Some(deadline) = state.pending.deadline() {
                    let now = Instant::now();
                    if state.closed || now >= deadline {
                        break;
                    }
                    (state, _) = shared.wake.wait_timeout(state, deadline - now).unwrap();
                } else if state.closed {
                    return;
                } else {
                    state = shared.wake.wait(state).unwrap();
                }
            }
            std::mem::take(&mut state.pending).changes
        };
        // At most four in-flight changes plus four pending slots. Neither I/O
        // nor logging holds the queue lock. Never restore a failed patch.
        let fields = changes.map(|change| change.is_some());
        let patch: Vec<_> = changes.into_iter().flatten().collect();
        let warning = match store.update_gui_preferences(&patch) {
            Ok(_) => None,
            Err(error) => {
                logger.failure("GUI preferences save failed", &error, &[]);
                Some(safe_warning(&error))
            }
        };
        let mut state = shared.lock_state();
        // Failure rejects the entire batch, including its otherwise valid fields.
        // Success only resolves fields in this patch, never older failed values.
        for (failure, included) in state.unsaved.iter_mut().zip(fields) {
            if included {
                *failure = warning;
            }
        }
        shared.publish(&state);
    }
}

fn safe_warning(error: &Error) -> &'static str {
    match error {
        Error::Busy => {
            "GUI preference save could not be confirmed: the configuration store is busy."
        }
        Error::Decode(_) => {
            "GUI preference save could not be confirmed: the preferences file could not be decoded."
        }
        Error::Encode(_) => {
            "GUI preference save could not be confirmed: the preferences could not be encoded."
        }
        Error::Invalid(_) => {
            "GUI preference save could not be confirmed: the preferences are invalid."
        }
        Error::Io(_) => {
            "GUI preference save could not be confirmed: a filesystem operation failed."
        }
        Error::Conflict(_) => {
            "GUI preference save could not be confirmed: the configuration changed in another process."
        }
        Error::Certificate { .. } | Error::Connection(_) | Error::Mapping(_) => {
            "GUI preference save could not be confirmed: a configuration operation failed."
        }
    }
}

#[cfg(test)]
mod tests;

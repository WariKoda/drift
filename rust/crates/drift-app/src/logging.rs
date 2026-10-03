//! Opt-in diagnostics. File I/O runs on a dedicated worker, never in a view.
//! Untrusted error messages (including decoded TOML and server replies) are not logged.
use drift_core::error::Error;
use std::{
    ffi::OsString,
    fs::{File, OpenOptions},
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread::JoinHandle,
};
use tokio::sync::watch;

#[derive(Debug, PartialEq, Eq)]
pub struct Options {
    pub path: PathBuf,
    pub debug: bool,
}
impl Options {
    /// Match Go: a nonempty flag path wins, debug is flag OR environment.
    pub fn resolve(
        flag_path: Option<PathBuf>,
        flag_debug: bool,
        env_path: Option<OsString>,
        env_debug: Option<OsString>,
        config_dir: &Path,
    ) -> Option<Self> {
        let path = flag_path
            .filter(|p| !p.as_os_str().is_empty())
            .or_else(|| env_path.filter(|p| !p.is_empty()).map(PathBuf::from));
        let debug = flag_debug
            || env_debug.is_some_and(|v| {
                !v.is_empty()
                    && !matches!(
                        v.to_str(),
                        Some("0" | "f" | "F" | "false" | "FALSE" | "False")
                    )
            });
        if path.is_none() && !debug {
            return None;
        }
        Some(Self {
            path: path.unwrap_or_else(|| config_dir.join("drift.log")),
            debug,
        })
    }
}

#[derive(Clone)]
pub struct Logger(Arc<Inner>);
struct Inner {
    sender: Mutex<Option<mpsc::Sender<String>>>,
    worker: Mutex<Option<JoinHandle<io::Result<()>>>>,
    failure: watch::Sender<Option<String>>,
    debug: bool,
}
impl Default for Logger {
    fn default() -> Self {
        Self::disabled(None)
    }
}
impl Logger {
    pub fn disabled(warning: Option<String>) -> Self {
        Self(Arc::new(Inner {
            sender: Mutex::new(None),
            worker: Mutex::new(None),
            failure: watch::channel(warning).0,
            debug: false,
        }))
    }
    pub fn open(options: Options) -> io::Result<Self> {
        if let Some(parent) = options.path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        // A mistaken FIFO path must not hang startup or the writer on shutdown.
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .custom_flags(nix::libc::O_NONBLOCK)
            .open(&options.path)?;
        use std::os::unix::fs::FileTypeExt;
        if file.metadata()?.file_type().is_fifo() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "log path is a FIFO",
            ));
        }
        let (sender, receiver) = mpsc::channel::<String>();
        let failure = watch::channel(None).0;
        let notify = failure.clone();
        let path = options.path;
        let worker = std::thread::Builder::new()
            .name("drift-log".into())
            .spawn(move || {
                let result = write_records(file, receiver);
                if let Err(error) = &result {
                    notify.send_replace(Some(format!(
                        "Could not write log file {}: {error}; logging disabled",
                        path.display()
                    )));
                }
                result
            })?;
        Ok(Self(Arc::new(Inner {
            sender: Mutex::new(Some(sender)),
            worker: Mutex::new(Some(worker)),
            failure,
            debug: options.debug,
        })))
    }
    /// The latest failure is retained even when it precedes GUI subscription.
    pub fn failures(&self) -> watch::Receiver<Option<String>> {
        self.0.failure.subscribe()
    }
    pub fn info(&self, event: &'static str, fields: &[(&str, &str)]) {
        self.record("INFO", event, fields);
    }
    pub fn debug(&self, event: &'static str, fields: &[(&str, &str)]) {
        if self.0.debug {
            self.record("DEBUG", event, fields);
        }
    }
    pub fn error(&self, event: &'static str, fields: &[(&str, &str)]) {
        self.record("ERROR", event, fields);
    }
    pub fn failure(&self, event: &'static str, error: &Error, fields: &[(&str, &str)]) {
        let kind = match error {
            Error::Invalid(_) => "invalid",
            Error::Certificate { .. } => "certificate",
            Error::Connection(_) => "connection",
            Error::Conflict(_) => "conflict",
            Error::Busy => "busy",
            Error::Io(_) => "io",
            Error::Decode(_) => "config_decode",
            Error::Encode(_) => "config_encode",
            Error::Mapping(_) => "mapping",
        };
        let mut context = fields.to_vec();
        context.push(("error_kind", kind));
        let io_kind;
        let os_error;
        if let Error::Io(error) = error {
            io_kind = format!("{:?}", error.kind());
            context.push(("io_kind", &io_kind));
            os_error = error.raw_os_error().map(|code| code.to_string());
            if let Some(code) = &os_error {
                context.push(("os_error", code));
            }
        }
        self.error(event, &context);
    }
    fn record(&self, level: &str, event: &str, fields: &[(&str, &str)]) {
        let sender = self.0.sender.lock().unwrap();
        let Some(sender) = sender.as_ref() else {
            return;
        };
        use std::fmt::Write;
        let mut record = format!(
            "time={} level={level} msg={event:?}",
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        );
        for (key, value) in fields {
            // Debug escaping keeps remote filenames and control characters on one line.
            let _ = write!(record, " {key}={value:?}");
        }
        record.push('\n');
        // A worker failure is reported through failures(), not to stdout/stderr.
        let _ = sender.send(record);
    }
    /// Drain, flush and close after the CLI command or GUI loop ends, not in Update.
    pub fn finish(&self) -> io::Result<()> {
        self.0.sender.lock().unwrap().take();
        if let Some(worker) = self.0.worker.lock().unwrap().take() {
            return worker
                .join()
                .map_err(|_| io::Error::other("log worker panicked"))?;
        }
        Ok(())
    }
}
fn write_records(mut file: File, receiver: mpsc::Receiver<String>) -> io::Result<()> {
    let written = (|| {
        for record in receiver {
            file.write_all(record.as_bytes())?;
        }
        file.flush()
    })();
    let closed =
        nix::unistd::close(file).map_err(|error| io::Error::from_raw_os_error(error as i32));
    written.and(closed)
}

#[cfg(test)]
mod tests;

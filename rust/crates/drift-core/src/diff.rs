//! Immutable comparison data and direction-aware unified rows. No UI or I/O.
pub mod hunks;
use similar::{ChangeTag, TextDiff};
use std::time::{Duration, SystemTime};

pub const TEXT_LIMIT: u64 = 2 * 1024 * 1024;
#[derive(Clone, Debug)]
pub struct FileMetadata {
    pub size: u64,
    pub modified: Option<SystemTime>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Equal,
    Added,
    Removed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub text: String,
    pub kind: LineKind,
    pub local: Option<usize>,
    pub remote: Option<usize>,
}
#[derive(Clone, Debug, Default)]
pub struct DiffResult {
    pub local: Option<FileMetadata>,
    pub remote: Option<FileMetadata>,
    pub binary: bool,
    pub content_diff: bool,
    pub lines: Vec<DiffLine>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Decision {
    #[default]
    Skip,
    Upload,
    Download,
    DeleteLocal,
    DeleteRemote,
}
impl Decision {
    pub fn label(self) -> &'static str {
        match self {
            Self::Skip => "Skip",
            Self::Upload => "Upload",
            Self::Download => "Download",
            Self::DeleteLocal => "Delete local",
            Self::DeleteRemote => "Delete remote",
        }
    }
}
impl DiffResult {
    pub fn differs(&self) -> bool {
        self.local.is_some() != self.remote.is_some()
            || self.content_diff
            || self.lines.iter().any(|l| l.kind != LineKind::Equal)
    }
    pub fn suggestion(&self) -> Decision {
        match (&self.local, &self.remote) {
            (Some(_), None) => Decision::Upload,
            (None, Some(_)) => Decision::Download,
            (Some(local), Some(remote)) if self.differs() => {
                // Unknown/close timestamps default to Upload, like the Go policy.
                if let (Some(local), Some(remote)) = (local.modified, remote.modified)
                    && remote
                        .duration_since(local)
                        .is_ok_and(|delta| delta > Duration::from_secs(2))
                {
                    Decision::Download
                } else {
                    Decision::Upload
                }
            }
            _ => Decision::Skip,
        }
    }
    pub fn next_decision(&self, current: Decision) -> Decision {
        match (&self.local, &self.remote, current) {
            (Some(_), None, Decision::Skip) => Decision::Upload,
            (Some(_), None, Decision::Upload) => Decision::DeleteLocal,
            (None, Some(_), Decision::Skip) => Decision::Download,
            (None, Some(_), Decision::Download) => Decision::DeleteRemote,
            (Some(_), Some(_), Decision::Skip) => Decision::Upload,
            (Some(_), Some(_), Decision::Upload) => Decision::Download,
            _ => Decision::Skip,
        }
    }
    /// Metadata and large-file digests are handled by the application layer.
    pub fn text(&mut self, local: &[u8], remote: &[u8]) {
        self.content_diff = local != remote;
        self.binary = local
            .iter()
            .take(512)
            .chain(remote.iter().take(512))
            .any(|b| *b == 0);
        if self.binary {
            return;
        }
        let diff = TextDiff::configure()
            .timeout(Duration::from_secs(2))
            .diff_lines(local, remote);
        let (mut local_num, mut remote_num) = (1, 1);
        for change in diff.iter_all_changes() {
            let kind = match change.tag() {
                ChangeTag::Equal => LineKind::Equal,
                ChangeTag::Delete => LineKind::Removed,
                ChangeTag::Insert => LineKind::Added,
            };
            let normalized = String::from_utf8_lossy(change.value())
                .replace("\r\n", "\n")
                .replace('\r', "\n");
            for text in normalized.split_terminator('\n') {
                let local = (kind != LineKind::Added).then_some(local_num);
                let remote = (kind != LineKind::Removed).then_some(remote_num);
                self.lines.push(DiffLine {
                    text: text.into(),
                    kind,
                    local,
                    remote,
                });
                local_num += usize::from(local.is_some());
                remote_num += usize::from(remote.is_some());
            }
        }
    }
}

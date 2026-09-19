use crate::fsops::SyncOutcome;
use crate::sync::Stats;

#[doc(hidden)]
pub enum ProgressMessage {
    DoneSyncing(SyncOutcome),
    StartSync {
        description: String,
        size: usize,
    },
    /// Snapshot of the source tree scan. Counts are absolute values,
    /// sent every `WALK_UPDATE_INTERVAL` entries and twice at the end
    /// (once without `finished`, then with it, so that the scanning
    /// progress line is displayed at least once, then erased).
    Walk {
        seen: u64,
        num_files: u64,
        total_size: usize,
        excluded_files: u64,
        excluded_dirs: u64,
        finished: bool,
    },
    Syncing {
        description: String,
        size: usize,
        done: usize,
    },
    SyncError {
        entry: String,
        details: String,
    },
}

/// Progress of the scan of the source tree.
#[derive(Debug)]
pub struct WalkInfo {
    /// Number of entries seen so far (files and directories)
    pub seen: u64,
    /// Number of files that passed the filters so far
    pub num_files: u64,
    /// Files excluded by the filters so far
    pub excluded_files: u64,
    /// Directories pruned by the filters so far
    pub excluded_dirs: u64,
    /// Whether the scan is finished
    pub finished: bool,
}

pub struct Progress {
    /// Name of the file being transferred
    pub current_file: String,
    /// Number of bytes transfered for the current file
    pub file_done: usize,
    /// Size of the current file (in bytes)
    pub file_size: usize,
    /// Number of bytes transfered since the start
    pub total_done: usize,
    /// Estimated total size of the transfer (this may change during transfer)
    pub total_size: usize,
    /// Index of the current file in the list of all files to transfer
    pub index: usize,
    /// Total number of files to transfer
    pub num_files: usize,
    /// Estimated time remaining for the transfer, in seconds
    pub eta: usize,
    /// Current transfer speed, in bytes per second (over a short sliding
    /// window)
    pub current_speed: usize,
    /// Average transfer speed since the start of the run, in bytes per second
    pub average_speed: usize,
    /// Time elapsed since the start of the current sync run, in seconds
    pub elapsed: usize,
}

/// Trait for implementing rusync progress details
pub trait ProgressInfo {
    /// A new transfer has begun from the `source` directory to the `destination`
    /// directory
    #[allow(unused_variables)]
    fn start(&mut self, source: &str, destination: &str) {}

    /// The source tree is being scanned
    #[allow(unused_variables)]
    fn scanning(&mut self, info: &WalkInfo) {}

    /// A new file named `name` is being transfered
    #[allow(unused_variables)]
    fn new_file(&mut self, name: &str) {}

    /// The file transfer is done
    #[allow(unused_variables)]
    fn done_syncing(&mut self) {}

    /// Callback for the detailed progress
    #[allow(unused_variables)]
    fn progress(&mut self, progress: &Progress) {}

    /// The transfer between `source` and `destination` is done. Details
    /// of the transfer in the Stats struct
    #[allow(unused_variables)]
    fn end(&mut self, stats: &Stats) {}

    /// The entry could not be synced
    #[allow(unused_variables)]
    fn error(&mut self, entry: &str, details: &str) {}
}

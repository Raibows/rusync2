use std::sync::mpsc::Receiver;
use std::time::Instant;

use crate::progress::{Progress, ProgressInfo, ProgressMessage, WalkInfo};
use crate::sync::Stats;

pub struct ProgressWorker {
    input: Receiver<ProgressMessage>,
    progress_info: Box<dyn ProgressInfo + Send>,
}

impl ProgressWorker {
    pub fn new(
        input: Receiver<ProgressMessage>,
        progress_info: Box<dyn ProgressInfo + Send>,
    ) -> ProgressWorker {
        ProgressWorker {
            input,
            progress_info,
        }
    }

    pub fn start(mut self) -> Stats {
        let mut stats = Stats::new();
        let mut file_done = 0;
        let mut current_file = String::from("");
        let mut index = 0;
        let mut total_done = 0;
        let now = Instant::now();
        stats.start();
        for progress in self.input.iter() {
            match progress {
                ProgressMessage::Walk {
                    seen,
                    num_files,
                    total_size,
                    excluded_files,
                    excluded_dirs,
                    finished,
                } => {
                    stats.num_files = num_files;
                    stats.total_size = total_size;
                    stats.excluded_files = excluded_files;
                    stats.excluded_dirs = excluded_dirs;
                    let info = WalkInfo {
                        seen,
                        num_files,
                        excluded_files,
                        excluded_dirs,
                        finished,
                    };
                    self.progress_info.scanning(&info);
                }
                ProgressMessage::StartSync { description, size } => {
                    self.progress_info.new_file(&description);
                    current_file = description;
                    index += 1;
                    // Render a per-file line right away, so that a run
                    // where every file is up to date still shows progress:
                    let elapsed = now.elapsed().as_secs() as usize;
                    let eta = compute_eta(elapsed, stats.total_size, total_done);
                    let detailed_progress = Progress {
                        file_done: 0,
                        file_size: size.max(1),
                        total_done,
                        total_size: stats.total_size,
                        index,
                        num_files: display_total(stats.num_files, index),
                        current_file: current_file.clone(),
                        eta,
                    };
                    self.progress_info.progress(&detailed_progress);
                }
                ProgressMessage::DoneSyncing(x) => {
                    self.progress_info.done_syncing();
                    stats.add_outcome(&x);
                    file_done = 0;
                }
                ProgressMessage::SyncError { entry, details } => {
                    self.progress_info.error(&entry, &details);
                    stats.add_error();
                }
                ProgressMessage::Syncing { done, size, .. } => {
                    file_done += done;
                    total_done += done;
                    let elapsed = now.elapsed().as_secs() as usize;
                    let eta = compute_eta(elapsed, stats.total_size, total_done);
                    let detailed_progress = Progress {
                        file_done,
                        file_size: size,
                        total_done,
                        total_size: stats.total_size,
                        index,
                        num_files: display_total(stats.num_files, index),
                        current_file: current_file.clone(),
                        eta,
                    };
                    self.progress_info.progress(&detailed_progress);
                }
            }
        }
        stats.stop();
        self.progress_info.end(&stats);
        stats
    }
}

/// The number of files to display: `num_files` may lag behind (walk
/// snapshots are batched), while `index` files are already known to exist.
fn display_total(num_files: u64, index: usize) -> usize {
    std::cmp::max(num_files as usize, index)
}

fn compute_eta(elapsed_secs: usize, total_size: usize, total_done: usize) -> usize {
    if total_done == 0 || total_size == 0 {
        return 0;
    }
    ((elapsed_secs * total_size) / total_done).saturating_sub(elapsed_secs)
}

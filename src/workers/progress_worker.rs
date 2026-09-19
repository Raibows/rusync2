use std::collections::VecDeque;
use std::sync::mpsc::Receiver;
use std::time::Duration;
use std::time::Instant;

use crate::progress::{Progress, ProgressInfo, ProgressMessage, WalkInfo};
use crate::sync::Stats;

/// Length of the sliding window used to compute the current speed
const SPEED_WINDOW: Duration = Duration::from_secs(2);
/// Minimum time span between the two samples used to compute a speed
const MIN_SPEED_SPAN: Duration = Duration::from_millis(100);
/// Maximum number of samples kept, so that very fast transfers (many
/// messages per second) do not grow the window unboundedly
const MAX_SPEED_SAMPLES: usize = 64;
/// Minimum delay between two recorded samples; with messages arriving
/// thousands of times per second, recording every message would shrink
/// the sliding window below `MIN_SPEED_SPAN`
const MIN_SAMPLE_INTERVAL: Duration = Duration::from_millis(100);

/// Computes the current transfer speed in bytes per second, from a
/// sliding window of (elapsed time, cumulative bytes done) samples.
struct SpeedMeter {
    samples: VecDeque<(Duration, u64)>,
    last_sample: Option<Duration>,
}

impl SpeedMeter {
    fn new() -> SpeedMeter {
        SpeedMeter {
            samples: VecDeque::new(),
            last_sample: None,
        }
    }

    /// Record that `total_done` bytes have been transferred `elapsed`
    /// after the start of the run, and return the current speed in bytes
    /// per second.
    fn push(&mut self, elapsed: Duration, total_done: u64) -> usize {
        if let Some(last) = self.last_sample {
            if elapsed.saturating_sub(last) < MIN_SAMPLE_INTERVAL {
                return self.speed();
            }
        }
        self.last_sample = Some(elapsed);
        self.samples.push_back((elapsed, total_done));
        while self.samples.len() > MAX_SPEED_SAMPLES {
            self.samples.pop_front();
        }
        while let Some((first, _)) = self.samples.front() {
            if elapsed.saturating_sub(*first) > SPEED_WINDOW {
                self.samples.pop_front();
            } else {
                break;
            }
        }
        self.speed()
    }

    /// The current speed in bytes per second, from the recorded samples.
    /// Samples with an unchanged byte count make the speed decay to 0
    /// when nothing is transferred.
    fn speed(&self) -> usize {
        match (self.samples.front(), self.samples.back()) {
            (Some((first_time, first_bytes)), Some((last_time, last_bytes))) => {
                let span = last_time.saturating_sub(*first_time);
                if span >= MIN_SPEED_SPAN {
                    let bytes = last_bytes.saturating_sub(*first_bytes);
                    (bytes as f64 / span.as_secs_f64()) as usize
                } else {
                    0
                }
            }
            _ => 0,
        }
    }
}

fn average_speed(total_done: usize, elapsed: Duration) -> usize {
    let secs = elapsed.as_secs_f64();
    if secs > 0.0 {
        (total_done as f64 / secs) as usize
    } else {
        0
    }
}

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
        let mut speed_meter = SpeedMeter::new();
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
                    let elapsed = now.elapsed();
                    let current_speed = speed_meter.push(elapsed, total_done as u64);
                    let detailed_progress = Progress {
                        file_done: 0,
                        file_size: size.max(1),
                        total_done,
                        total_size: stats.total_size,
                        index,
                        num_files: display_total(stats.num_files, index),
                        current_file: current_file.clone(),
                        eta: compute_eta(elapsed.as_secs() as usize, stats.total_size, total_done),
                        current_speed,
                        average_speed: average_speed(total_done, elapsed),
                        elapsed: elapsed.as_secs() as usize,
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
                    let elapsed = now.elapsed();
                    let current_speed = speed_meter.push(elapsed, total_done as u64);
                    let detailed_progress = Progress {
                        file_done,
                        file_size: size,
                        total_done,
                        total_size: stats.total_size,
                        index,
                        num_files: display_total(stats.num_files, index),
                        current_file: current_file.clone(),
                        eta: compute_eta(elapsed.as_secs() as usize, stats.total_size, total_done),
                        current_speed,
                        average_speed: average_speed(total_done, elapsed),
                        elapsed: elapsed.as_secs() as usize,
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

#[cfg(test)]
mod tests {
    use super::SpeedMeter;
    use std::time::Duration;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn speed_meter_computes_bytes_per_second() {
        let mut meter = SpeedMeter::new();
        assert_eq!(meter.push(secs(0), 0), 0);
        assert_eq!(meter.push(secs(1), 1000), 1000);
        assert_eq!(meter.push(secs(2), 2000), 1000);
    }

    #[test]
    fn speed_meter_evicts_samples_older_than_the_window() {
        let mut meter = SpeedMeter::new();
        meter.push(secs(0), 0);
        meter.push(secs(1), 100);
        meter.push(secs(2), 200);
        // At t=3 the (0, 0) sample left the 2s window:
        assert_eq!(meter.push(secs(3), 300), 100);
    }

    #[test]
    fn speed_meter_returns_zero_for_short_spans() {
        let mut meter = SpeedMeter::new();
        meter.push(Duration::from_millis(0), 0);
        assert_eq!(meter.push(Duration::from_millis(50), 1000), 0);
    }

    #[test]
    fn speed_meter_caps_the_number_of_samples() {
        let mut meter = SpeedMeter::new();
        for i in 0..200 {
            meter.push(Duration::from_millis(i), i);
        }
        assert!(meter.samples.len() <= 64);
    }

    #[test]
    fn speed_meter_decays_when_nothing_is_transferred() {
        let mut meter = SpeedMeter::new();
        meter.push(secs(0), 0);
        meter.push(secs(1), 1000);
        // No new bytes for a while: the speed drops to 0:
        assert_eq!(meter.push(Duration::from_millis(2500), 1000), 0);
    }
}

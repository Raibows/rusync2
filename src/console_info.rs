//! console_info
//!
//! Display transfer progress to the command line

use crate::progress::{Progress, ProgressInfo, WalkInfo};
use crate::sync;
use anyhow::{Context, Error};
use colored::Colorize;
use humansize::{file_size_opts as options, FileSize};
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::path::Path;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use terminal_size::{terminal_size, Width};

/// Minimum delay between two rendered progress lines. Rendering on every
/// event is both slow (terminal writes) and flickery; for trees with many
/// small files the events come in thousands per second.
const MIN_RENDER_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub struct ConsoleProgressInfo {
    err_file: Option<std::fs::File>,
    last_render: Option<Instant>,
    last_scan_render: Option<Instant>,
    line_dirty: bool,
}

impl ConsoleProgressInfo {
    pub fn new() -> Self {
        Self {
            err_file: None,
            last_render: None,
            last_scan_render: None,
            line_dirty: false,
        }
    }

    pub fn with_error_list_path(error_list_path: &Path) -> Result<Self, Error> {
        let err_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(error_list_path)
            .with_context(|| {
                format!("Could not open errfile at '{}'", error_list_path.display())
            })?;
        Ok(Self {
            err_file: Some(err_file),
            last_render: None,
            last_scan_render: None,
            line_dirty: false,
        })
    }

    /// Whether a transfer progress line should be rendered now; also
    /// updates the render clock. The first line is always rendered.
    fn should_render(&mut self) -> bool {
        let ready = match self.last_render {
            None => true,
            Some(last) => last.elapsed() >= MIN_RENDER_INTERVAL,
        };
        if ready {
            self.last_render = Some(Instant::now());
        }
        ready
    }

    /// Same as `should_render`, but for the scanning line, so that a busy
    /// transfer phase cannot starve the scanning display (and the first
    /// scanning line is always rendered).
    fn should_render_scan(&mut self) -> bool {
        let ready = match self.last_scan_render {
            None => true,
            Some(last) => last.elapsed() >= MIN_RENDER_INTERVAL,
        };
        if ready {
            self.last_scan_render = Some(Instant::now());
        }
        ready
    }

    fn erase_line_if_dirty(&mut self) {
        if self.line_dirty {
            erase_line();
            self.line_dirty = false;
        }
    }
}

impl ProgressInfo for ConsoleProgressInfo {
    fn done_syncing(&mut self) {
        self.erase_line_if_dirty();
    }

    fn start(&mut self, source: &str, destination: &str) {
        println!(
            "{} Syncing from {} to {} …",
            "::".color("blue"),
            source.bold(),
            destination.bold()
        )
    }

    fn new_file(&mut self, _name: &str) {}

    fn scanning(&mut self, info: &WalkInfo) {
        if info.finished {
            self.erase_line_if_dirty();
            return;
        }
        if !self.should_render_scan() {
            return;
        }
        // Pad to the terminal width so that a previous, longer progress
        // line is fully overwritten:
        let line = format!(
            "{:<width$}",
            scanning_line(info),
            width = get_terminal_width()
        );
        print!("{}\r", line);
        let _ = io::stdout().flush();
        self.line_dirty = true;
    }

    fn progress(&mut self, progress: &Progress) {
        if !self.should_render() {
            return;
        }
        let eta_str = human_seconds(progress.eta);
        let percent_width = 3;
        let eta_width = eta_str.len();
        let index = progress.index;
        let index_width = index.to_string().len();
        let num_files = progress.num_files;
        let num_files_width = num_files.to_string().len();
        let widgets_width = percent_width + index_width + num_files_width + eta_width;
        let num_separators = 5;
        let line_width = get_terminal_width();
        let file_width = line_width - widgets_width - num_separators - 1;
        let current_file = progress.current_file.clone();
        let current_file = truncate_lossy(&current_file, file_width);
        let current_file = format!(
            "{filename:<pad$}",
            pad = file_width,
            filename = current_file
        );
        let file_percent = (progress.file_done * 100) / progress.file_size.max(1);
        print!(
            "{:>3}% {}/{} {} {:<}\r",
            file_percent, index, num_files, current_file, eta_str
        );
        let _ = io::stdout().flush();
        self.line_dirty = true;
    }

    fn error(&mut self, entry: &str, desc: &str) {
        eprintln!("Errror: {}", desc);
        if let Some(err_file) = &mut self.err_file {
            // Ignoring errrors when trying to log errors ...
            let _ = err_file.write(entry.as_bytes());
            let _ = err_file.write(b"\n");
            let _ = err_file.flush();
        }
    }

    fn end(&mut self, stats: &sync::Stats) {
        println!(
            "{} Synced {} files ({} up to date)",
            " ✓".color("green"),
            stats.num_synced,
            stats.up_to_date
        );
        println!(
            "{} files copied, {} symlinks created, {} symlinks updated",
            stats.copied, stats.symlink_created, stats.symlink_updated
        );
        if stats.excluded_files > 0 || stats.excluded_dirs > 0 {
            println!(
                "{} files skipped by filters ({} directories pruned)",
                stats.excluded_files, stats.excluded_dirs
            );
        }
        let transfered = stats.total_transfered;
        // We know transfered cannot be negative
        let transfered = transfered.file_size(options::DECIMAL).unwrap();
        let duration = stats.duration();
        // Truncate below 1 second
        let duration = std::time::Duration::from_secs(duration.as_secs());
        let duration = humantime::format_duration(duration);
        println!("{} copied in {}", transfered, duration);
        if stats.errors != 0 {
            eprintln!("{} errors occurred", stats.errors);
        }
    }
}

impl Default for ConsoleProgressInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Print the time the sync completed and the time of the next sync, in watch
/// mode (see the `--sleep-interval` option).
pub fn announce_next_sync(finished_at: &str, next_sync_at: &str) {
    println!(
        "{} Sync completed at {}, next sync at {}",
        "::".color("blue"),
        finished_at.bold(),
        next_sync_at.bold()
    );
}

/// One line of the progress bar displayed while sleeping between two syncs.
pub fn sleep_progress_line(
    elapsed_secs: u64,
    total_secs: u64,
    next_sync_at: &str,
    bar_width: usize,
) -> String {
    // The interval is at least one second, so this is never zero:
    let total = total_secs.max(1) as u128;
    let elapsed = elapsed_secs as u128;
    let percent = (elapsed * 100 / total) as u64;
    let filled = ((elapsed * bar_width as u128 / total) as usize).min(bar_width);
    let mut bar = String::new();
    for i in 0..bar_width {
        if i < filled {
            bar.push('#');
        } else {
            bar.push('.');
        }
    }
    format!(
        "waiting [{}] {:>3}% ({}/{}) next sync at {}",
        bar,
        percent,
        human_seconds(elapsed_secs as usize),
        human_seconds(total_secs as usize),
        next_sync_at
    )
}

/// Sleep for `interval`, refreshing `sleep_progress_line` while waiting.
pub fn sleep_with_progress(interval: Duration, next_sync_at: &str) {
    let start = Instant::now();
    let total = interval.as_secs().max(1);
    loop {
        let elapsed = start.elapsed();
        if elapsed >= interval {
            break;
        }
        let line = sleep_progress_line(elapsed.as_secs(), total, next_sync_at, 10);
        print!("{}\r", line);
        let _ = io::stdout().flush();
        thread::sleep(Duration::from_secs(1));
    }
    erase_line();
}

/// The line displayed while the source tree is being scanned.
fn scanning_line(info: &WalkInfo) -> String {
    let mut line = format!(
        "scanning … {} entries, {} to sync",
        info.seen, info.num_files
    );
    if info.excluded_files > 0 || info.excluded_dirs > 0 {
        line.push_str(&format!(
            ", {} skipped by filters",
            info.excluded_files + info.excluded_dirs
        ));
    }
    line
}

fn get_terminal_width() -> usize {
    if let Some((Width(w), _)) = terminal_size() {
        return w as usize;
    }
    // We're likely not a tty here, so this is a good enough default:
    80
}

fn erase_line() {
    let line_width = get_terminal_width();
    let line = vec![32_u8; line_width];
    // We're calling from_utf8 on a string containing only spaces,
    // so calling unwrap() is safe
    print!("{}\r", String::from_utf8(line).unwrap());
}

fn human_seconds(s: usize) -> String {
    let hours = s / 3600;
    let minutes = (s / 60) % 60;
    let seconds = s % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

fn truncate_lossy(text: &str, maxsize: usize) -> String {
    // Our goal here is to make sure the text can be written
    // in the terminal without going over the `maxsize` length
    // Our approach is to first convert to bytes, then truncate
    // the vector of bytes, then convert to a lossy string
    // This way we *know* we won't cut at a char boundary
    let mut as_bytes = text.to_string().into_bytes();
    as_bytes.truncate(maxsize);
    String::from_utf8_lossy(&as_bytes).to_string()
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn test_truncate_string() {
        let new_text = truncate_lossy("ééé", 2);
        assert_eq!(new_text, "é");
    }

    #[test]
    fn scanning_line_without_filters() {
        let info = WalkInfo {
            seen: 12,
            num_files: 10,
            excluded_files: 0,
            excluded_dirs: 0,
            finished: false,
        };
        assert_eq!(scanning_line(&info), "scanning … 12 entries, 10 to sync");
    }

    #[test]
    fn scanning_line_with_filters() {
        let info = WalkInfo {
            seen: 15,
            num_files: 10,
            excluded_files: 2,
            excluded_dirs: 1,
            finished: false,
        };
        assert_eq!(
            scanning_line(&info),
            "scanning … 15 entries, 10 to sync, 3 skipped by filters"
        );
    }

    #[test]
    fn test_human_seconds() {
        assert_eq!("00:00:05", human_seconds(5));
        assert_eq!("00:00:42", human_seconds(42));
        assert_eq!("00:03:05", human_seconds(185));
        assert_eq!("02:04:05", human_seconds(7445));
        assert_eq!("200:00:02", human_seconds(720_002));
    }

    #[test]
    fn sleep_progress_line_at_start() {
        let line = sleep_progress_line(0, 60, "2026-08-20 17:32:00", 10);
        assert_eq!(
            line,
            "waiting [..........]   0% (00:00:00/00:01:00) next sync at 2026-08-20 17:32:00"
        );
    }

    #[test]
    fn sleep_progress_line_half_way() {
        let line = sleep_progress_line(30, 60, "2026-08-20 17:32:00", 10);
        assert_eq!(
            line,
            "waiting [#####.....]  50% (00:00:30/00:01:00) next sync at 2026-08-20 17:32:00"
        );
    }

    #[test]
    fn sleep_progress_line_rounds_the_bar_down() {
        let line = sleep_progress_line(45, 120, "2026-08-20 17:32:00", 4);
        assert_eq!(
            line,
            "waiting [#...]  37% (00:00:45/00:02:00) next sync at 2026-08-20 17:32:00"
        );
    }

    #[test]
    fn sleep_progress_line_complete() {
        let line = sleep_progress_line(60, 60, "2026-08-20 17:32:00", 10);
        assert_eq!(
            line,
            "waiting [##########] 100% (00:01:00/00:01:00) next sync at 2026-08-20 17:32:00"
        );
    }

    #[test]
    fn sleep_progress_line_uses_hours() {
        let line = sleep_progress_line(0, 24 * 3600, "2026-08-21 17:32:00", 10);
        assert_eq!(
            line,
            "waiting [..........]   0% (00:00:00/24:00:00) next sync at 2026-08-21 17:32:00"
        );
    }
}

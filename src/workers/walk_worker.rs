use std::fs;
use std::fs::DirEntry;
use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use anyhow::{bail, Context, Error};

use crate::entry::Entry;
use crate::filters::Filters;
use crate::fsops;
use crate::progress::ProgressMessage;

/// Number of entries between two `Walk` progress snapshots
const WALK_UPDATE_INTERVAL: u64 = 32;

pub struct WalkWorker {
    entry_output: Sender<Entry>,
    progress_output: Sender<ProgressMessage>,
    source: PathBuf,
    filters: Filters,
}

impl WalkWorker {
    pub fn new(
        source: &Path,
        entry_output: Sender<Entry>,
        progress_output: Sender<ProgressMessage>,
        filters: Filters,
    ) -> WalkWorker {
        WalkWorker {
            entry_output,
            progress_output,
            source: source.to_path_buf(),
            filters,
        }
    }

    fn send_walk(
        &self,
        seen: u64,
        num_files: u64,
        total_size: usize,
        excluded_files: u64,
        excluded_dirs: u64,
        finished: bool,
    ) -> Result<(), Error> {
        let sent = self.progress_output.send(ProgressMessage::Walk {
            seen,
            num_files,
            total_size,
            excluded_files,
            excluded_dirs,
            finished,
        });
        if sent.is_err() {
            bail!("stats output chan is closed");
        }
        Ok(())
    }

    fn walk(&self) -> Result<(), Error> {
        let mut subdirs: Vec<PathBuf> = vec![self.source.to_path_buf()];
        let mut seen: u64 = 0;
        let mut num_files: u64 = 0;
        let mut total_size: usize = 0;
        let mut excluded_files: u64 = 0;
        let mut excluded_dirs: u64 = 0;
        while let Some(subdir) = subdirs.pop() {
            // We just checked that subdirs is *not* empty, so calling pop() is safe

            let entries = fs::read_dir(&subdir).with_context(|| {
                format!(
                    "While walking source, could not read directory '{}'",
                    subdir.display()
                )
            })?;
            for entry in entries {
                let entry = entry.with_context(|| {
                    format!(
                        "While walking source dir, could not read subdir: '{}'",
                        subdir.display()
                    )
                })?;
                let path = entry.path();
                // `file_type()` comes from the directory entry itself on
                // most platforms; only symlinks need an extra stat, to
                // preserve the "follow symlinks to directories" behavior.
                let file_type = entry.file_type().with_context(|| {
                    format!(
                        "While walking source, could not read the type of '{}'",
                        path.display()
                    )
                })?;
                let is_dir = if file_type.is_symlink() {
                    path.is_dir()
                } else {
                    file_type.is_dir()
                };
                let rel_path = fsops::get_rel_path(&path, &self.source);
                if is_dir {
                    if self.filters.dir_pruned(&rel_path) {
                        excluded_dirs += 1;
                    } else {
                        subdirs.push(path);
                    }
                } else if self.filters.passes(&rel_path) {
                    let size = self.process_file(&rel_path, &entry)?;
                    num_files += 1;
                    total_size += size;
                } else {
                    excluded_files += 1;
                }
                seen += 1;
                if seen.is_multiple_of(WALK_UPDATE_INTERVAL) {
                    self.send_walk(
                        seen,
                        num_files,
                        total_size,
                        excluded_files,
                        excluded_dirs,
                        false,
                    )?;
                }
            }
        }
        // Final updates: one snapshot so that the scanning line is displayed
        // at least once even on small trees, then a message marking the scan
        // as finished so that the scanning line can be erased.
        self.send_walk(
            seen,
            num_files,
            total_size,
            excluded_files,
            excluded_dirs,
            false,
        )?;
        self.send_walk(
            seen,
            num_files,
            total_size,
            excluded_files,
            excluded_dirs,
            true,
        )?;
        Ok(())
    }

    fn process_file(&self, rel_path: &Path, entry: &DirEntry) -> Result<usize, Error> {
        let desc = rel_path.to_string_lossy();
        let path = entry.path();
        // Fast path for regular files: reuse the metadata that the
        // directory entry already read (one syscall on platforms where
        // `DirEntry::metadata()` is not cached).
        let src_entry = match entry.metadata() {
            Ok(metadata) if !metadata.file_type().is_symlink() => {
                Entry::regular_file(&desc, &path, metadata)
            }
            _ => Entry::new(&desc, &path),
        };
        let metadata = src_entry
            .metadata()
            .with_context(|| format!("Could not read metadata from {:?}", path))?;
        self.entry_output
            .send(src_entry.clone())
            .with_context(|| "When walking source dir: could not send entry to progress worker")?;
        Ok(metadata.len() as usize)
    }

    pub fn start(&self) {
        let outcome = &self.walk();
        if outcome.is_err() {
            // Send err to output
        }
    }
}

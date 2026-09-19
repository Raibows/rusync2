use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};

use anyhow::{Context, Error};

use crate::entry::Entry;
use crate::fsops;
use crate::fsops::SyncOutcome;
use crate::progress::ProgressMessage;
use crate::sync::SyncOptions;

pub struct SyncWorker {
    input: Receiver<Entry>,
    output: Sender<ProgressMessage>,
    source: PathBuf,
    destination: PathBuf,
    /// Parent directory of the last entry, so that `create_dir_all` runs
    /// once per directory instead of once per file.
    last_parent: Option<PathBuf>,
    /// Reused for every copied file, instead of allocating a fresh buffer
    /// per file (which matters for trees with many small files).
    copy_buffer: Vec<u8>,
}

impl SyncWorker {
    pub fn new(
        source: &Path,
        destination: &Path,
        input: Receiver<Entry>,
        output: Sender<ProgressMessage>,
    ) -> SyncWorker {
        SyncWorker {
            input,
            output,
            source: source.to_path_buf(),
            destination: destination.to_path_buf(),
            last_parent: None,
            copy_buffer: vec![0; fsops::BUFFER_SIZE],
        }
    }

    pub fn start(mut self, opts: SyncOptions) -> Result<(), Error> {
        // `while let` ends the borrow of `self.input` before
        // `self.sync(&mut self, ...)`:
        while let Ok(entry) = self.input.recv() {
            let sync_outcome = self.sync(&entry, &opts);
            let progress_message = match sync_outcome {
                Ok(s) => ProgressMessage::DoneSyncing(s),
                Err(e) => ProgressMessage::SyncError {
                    entry: entry.description().to_string(),
                    details: format!("{:#}", e),
                },
            };
            self.output.send(progress_message)?;
        }
        Ok(())
    }

    fn create_missing_dest_dirs(&mut self, rel_path: &Path) -> Result<(), Error> {
        let parent_rel_path = rel_path
            .parent()
            .expect("dest directory should have a parent");
        if self.last_parent.as_deref() == Some(parent_rel_path) {
            return Ok(());
        }
        let to_create = self.destination.join(parent_rel_path);
        fs::create_dir_all(&to_create)
            .with_context(|| format!("Could not create '{}'", to_create.display()))?;
        self.last_parent = Some(parent_rel_path.to_path_buf());
        Ok(())
    }

    fn sync(&mut self, src_entry: &Entry, opts: &SyncOptions) -> Result<SyncOutcome, Error> {
        let rel_path = fsops::get_rel_path(src_entry.path(), &self.source);
        self.create_missing_dest_dirs(&rel_path)?;
        let desc = rel_path.to_string_lossy();

        let dest_path = self.destination.join(&rel_path);
        let dest_entry = Entry::new(&desc, &dest_path);
        let outcome =
            fsops::sync_entries(&self.output, src_entry, &dest_entry, &mut self.copy_buffer)?;
        #[cfg(unix)]
        {
            if opts.preserve_permissions {
                fsops::copy_permissions(src_entry, &dest_entry)?;
            }
        }
        Ok(outcome)
    }
}

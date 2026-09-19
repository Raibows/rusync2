# 0.10.0

* Show the current transfer speed and the average speed since the start
  of the run in the progress line, and the average speed in the final
  summary (`40 KB copied in 2s (avg 20 B/s)`). The current speed is
  computed over a 2-second sliding window and decays to 0 when nothing
  is transferred.

* For library users: the `Progress` struct gained `current_speed` and
  `average_speed` fields, in bytes per second.

# 0.9.0

* Show progress at every stage: a scanning line while the source tree is
  walked (with the number of entries seen and skipped by filters), a
  per-file line while files are compared — so that a run where everything
  is up to date still shows progress — and the `:: Syncing from …` header
  line, which was previously never printed. Rendering is throttled to 10
  frames per second.

* Speed up syncing trees with many small files: one `lstat` per entry
  instead of up to three, file types read from the directory entry,
  destination directories created once per directory instead of once per
  file, a reused copy buffer, and throttled progress rendering. On a tree
  with 40k small files: fresh copy 3.04s → 2.50s, everything up to date
  1.21s → 0.91s.

* For library users: `ProgressInfo` gained a `scanning(&WalkInfo)`
  callback (default no-op), `ProgressMessage::Todo` and
  `ProgressMessage::Excluded` were replaced by `ProgressMessage::Walk`
  snapshots, and `StartSync` now carries the file size.

* Update dependencies.

# 0.8.0

* Fork of the unmaintained [rusync](https://github.com/dmerejkowsky/rusync),
  published to crates.io as `rusync2`. The package installs both the
  `rusync` and the `rusync2` commands; the library is unchanged.

* Add `--include` and `--exclude` options. Both take regular expressions, can
  be repeated, and are matched against the path relative to the source
  directory (components joined by `/`). If any `--include` is given, only
  matching paths are synchronized; `--exclude` always takes precedence.
  Directories matching an exclude pattern are not descended into.

  For library users, `SyncOptions` gained a `filters` field, so struct
  literal constructions need updating (or use `..Default::default()`);
  `SyncOptions` is no longer `Copy`.

* Add a `--sleep-interval` option: after each sync, sleep for the given
  interval, then sync again until interrupted. Plain numbers mean seconds;
  suffixes `s`, `m`, `h`, `d` and combinations like `1h30m` are supported.
  While waiting, `rusync` prints the time the sync completed, the time of
  the next sync, and a progress bar.

* `rusync --help` and `rusync -h` now describe the program, the
  source/destination arguments, and the path matching and watch mode
  semantics.

* Show progress at every stage: a scanning line while the source tree is
  walked (with the number of entries seen and skipped by filters), a
  per-file line while files are compared — so that a run where everything
  is up to date still shows progress — and the `:: Syncing from …` header
  line, which was previously never printed.

* Speed up syncing trees with many small files: one `lstat` per entry
  instead of up to three, file types read from the directory entry,
  destination directories created once per directory instead of once per
  file, a reused copy buffer, and progress rendering throttled to 10
  frames per second instead of one write per event.

# 0.7.2

* Update dependencies

# 0.7.1

* Update author email
* Development branch is now called 'main'

# v0.7.0

* Switch to anyhow for error handling. This means you can use the
  alternate formatting (`{#?}`) to get the *cause* of each error

# v0.6.0

* Handle errors during syncing rather than aborting the whole process
* Add a `--errlist` option to record errors in the given file
* Display size and time of transfer in human-readable strings at the end
  of the transfer

## Changes in the API

* **breaking** The ProgressInfo now uses `&mut self`.

* **breakig** In order to handle errors during syncing, you should implement the
  `ProgressInfo::error()` method instead on relying on the returned
  value of `Syncer::sync()`.

* The `Stats` structs now also contains:
  * The duration of the transfer
  * The number of bytes written
  * The number of entries that could not be synced


# v0.5.3

* Cleanup README, command line options, project description and so on.
* Fix Clippy warnings
* Fix deprecated syntax

# v0.5.2

* Add Windows support
* Use 2018 edition
* Improve error handling. For instance, when an I/O error occurs, rusync always
  prints the filename that triggered it.
* Fix rare crash when displaying progress

# v0.5.1

* Fix misleading error message. Patch by @danieldulaney.

# v0.5.0

* rusync is now usable as a library! Thanks @mmstick for the suggestion. See [documentation](https://docs.rs/rusync) for details.

# v0.4.3

* Using term_size instead of terminal_size. This fixes compilation on Android.

# v0.4.2

* Bug fix: broken symlink in source directory were not re-created in the destination directory

# v0.4.1

* Improve error handling: display more details about the file operation that failed
  instead of just the raw io::Error

# v0.4.0

* Display an ETA at the right of the progress bar.

# v0.3.1

* Exit early if the source given on the command line is not an argument. We used to display a weird
  "0 files copied" in this case.

# v0.3.0

* Change output to be like a Ninja. Print all progress on one line, and erase it when done.

The line looks like:

```
 50% 24/50 Downloads/archlinux.iso
```

It contains the percentage of the current file that has been transfered, the index of the current transfered,
the total number of files to copy, and the name of the current file.

Note that the number of files to copy may increase while rusync is running: this is because the contents
of the source folder are read *while the copy is done*.


# v0.2.3

* Add a `--no-perms` flag to disable preservation of permissions. Useful when
  you *know* this will fail and don't want to be flooded with warning messages.

# v0.2.1

This contains several bug fixes regarding symlinks.

Here the algorithm when now use:

* If the destination does not exists:
  * Create a new symlink with the same target as the previous one.

* If the destination exists:

  * If it's not a symlink:
      * Abort!

  * Otherwise:

    * If the destination symlink already has the correct target, consider it up to date.
    * If the destination symlink is broken, remove it and re-create it.
    * If the destination symlink does not point to the correct location, remove it and re-create it.

# v0.2.0

* Try and preserve permissions after files are copied

# v0.1.2

* Add missing call to `stdout().flush()`

# v0.1.1

* Display a progress bar for each file

# v0.1.0

Initial release

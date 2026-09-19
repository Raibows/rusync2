# rusync2

Minimalist `rsync` implementation in Rust.

This is a maintained fork of the unmaintained
[dmerejkowsky/rusync](https://github.com/dmerejkowsky/rusync). The package
is published to crates.io as `rusync2` and installs both the `rusync` and
the `rusync2` commands.

# Usage

```
$ cargo install rusync2
$ rusync test/src test/dest
:: Syncing from test/src to test/dest …
scanning … 42 entries, 24 to sync
 50% 13/24 Downloads/archlinux.iso 1.07 GB 1.18 GB/s avg 1.16 GB/s elapsed 00:01:23 eta 00:00:05
```

# Caveat

We do everything we can to make sure data loss is impossible, but despite our best efforts, it may still happen.

Please make sure your files files are backed up if necessary before using `rusync` on sensitive data.

Thank you for your understanding!

# Features

* Easy to remember command line syntax.

* Print progress on one line at every stage — while scanning the source
  tree, while comparing files, and while copying — with the transferred
  size, the current and average speed, and the elapsed time of the
  current run, and erase it when done, thus avoiding flooding your
  terminal with useless noise.

* Displays a reliable ETA, without sacrificing speed.

* Few syscalls per entry, so that syncing trees with many small files
  stays fast.

* Unsurprising behavior: missing directories are created
  on the fly, files are only copied if:

  * destination is missing
  * destination exists but is older than the source
  * or source and destination have different sizes

# Command line options

* `--no-perms`: prevents `rusync` from trying to preserve file permissions (useful if you copy data from a Linux partition to NTFS for instance).
* `--err-list FILE`: write name of entries that caused errors in the given file, separated by `\n`
* `--include REGEX`: only synchronize paths matching this regex (may be repeated)
* `--exclude REGEX`: skip paths matching this regex (may be repeated)
* `--sleep-interval INTERVAL`: after each sync, sleep for INTERVAL, then sync
  again until interrupted. Plain numbers mean seconds; suffixes `s`, `m`, `h`,
  `d` and combinations like `1h30m` are supported.

# Path matching

`--include` and `--exclude` take regular expressions, matched against the
path relative to the source directory, with path components joined by `/`
(everywhere, including on Windows). Matching is case-sensitive and
unanchored: use `^` and `$` to anchor.

Rules:

* If any `--include` is given, only paths matching at least one of them
  are synchronized.
* A path matching any `--exclude` is always skipped; `--exclude` takes
  precedence over `--include`.
* A directory matching an exclude pattern is not descended into. Include
  patterns never prevent descent, since files below a directory may still
  match them.

Examples:

```
rusync xxx yyy --exclude '\.log$'                        # skip every .log file
rusync xxx yyy --exclude '(^|/)target$'                   # skip every target/ directory
rusync xxx yyy --include '^docs/' --exclude '\.tmp$'    # only docs/, without .tmp files
```

# Watch mode

Passing `--sleep-interval` makes `rusync` sync, then sleep, then sync again,
until you interrupt it with `Ctrl-C`. After every round, `rusync` prints the
time the sync completed and the time of the next sync, and shows a progress
bar while waiting:

```
 ✓ Synced 4 files (0 up to date)
 :: Sync completed at 2026-08-20 17:30:00, next sync at 2026-08-20 17:32:00
 waiting [#####.....]  50% (00:01:00/00:02:00) next sync at 2026-08-20 17:32:00
```

A plain number means seconds; `s`, `m`, `h` and `d` suffixes are supported,
and groups are summed, so all of these are valid intervals: `30`, `10m`,
`3h`, `1d`, `1h30m`.


# State of the project

I consider this project *done* - I don't intend on adding new features. The goal was to learn more about Rust and I've learned plenty of things already. If there's a feature present in `rsync` that is not available in `rusync`, just use `rsync`  - or try and implement the feature yourself, I'll be happy to review and merge your changes :)

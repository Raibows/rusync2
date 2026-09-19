use anyhow::{Context, Error};
use chrono::{DateTime, Local};
use clap::Parser;
use rusync::console_info::announce_next_sync;
use rusync::console_info::sleep_with_progress;
use rusync::console_info::ConsoleProgressInfo;
use rusync::parse_duration;
use rusync::sync::Stats;
use rusync::sync::SyncOptions;
use rusync::Filters;
use rusync::Syncer;
use std::path::PathBuf;
use std::process;
use std::time::Duration;

const AFTER_HELP: &str = "\
PATH MATCHING:
    --include and --exclude take regular expressions, matched against the path
    relative to SOURCE, with components joined by '/'. Matching is
    case-sensitive and unanchored: use ^ and $ to anchor.

    * If any --include is given, only paths matching at least one of them
      are synchronized.
    * A path matching any --exclude is always skipped; --exclude wins over
      --include.
    * A directory matching an --exclude is not descended into.

    Examples:
        rusync src dest --exclude '\\.log$'
        rusync src dest --include '^docs/' --exclude '\\.tmp$'

WATCH MODE:
    With --sleep-interval, rusync syncs, sleeps for the interval, then syncs
    again until interrupted with Ctrl-C. While waiting, it prints the time
    of the next sync and a progress bar. A plain number means seconds;
    suffixes s, m, h, d and combinations like 1h30m are accepted.";

/// Minimalist rsync clone in Rust.
#[derive(Debug, Parser)]
#[clap(name = "rusync", after_help = AFTER_HELP)]
struct Opt {
    #[clap(
        long = "no-perms",
        help = "Do not preserve permissions (no-op on Windows)"
    )]
    no_preserve_permissions: bool,

    #[clap(long = "err-list", help = "Write errors to the given file")]
    error_list_path: Option<PathBuf>,

    #[clap(
        long = "include",
        value_name = "REGEX",
        multiple_occurrences = true,
        help = "Only sync paths matching this regex (may be repeated)"
    )]
    include: Vec<String>,

    #[clap(
        long = "exclude",
        value_name = "REGEX",
        multiple_occurrences = true,
        help = "Skip paths matching this regex (may be repeated)"
    )]
    exclude: Vec<String>,

    #[clap(
        long = "sleep-interval",
        value_name = "INTERVAL",
        help = "After each sync, sleep for INTERVAL then sync again until \
                interrupted. Plain numbers mean seconds; suffixes: s, m, h, \
                d (e.g. 10, 10m, 3h, 1h30m)"
    )]
    sleep_interval: Option<String>,

    #[clap(parse(from_os_str), help = "Source directory to synchronize")]
    source: PathBuf,

    #[clap(
        parse(from_os_str),
        help = "Destination directory root: every path relative to SOURCE is \
                written to the same relative path under DESTINATION"
    )]
    destination: PathBuf,
}

fn make_options(opt: &Opt) -> Result<SyncOptions, Error> {
    let filters = Filters::new(&opt.include, &opt.exclude)?;
    Ok(SyncOptions {
        preserve_permissions: !opt.no_preserve_permissions,
        filters,
    })
}

fn sync_once(opt: &Opt, options: &SyncOptions) -> Result<Stats, Error> {
    let console_info = match &opt.error_list_path {
        Some(err_file) => ConsoleProgressInfo::with_error_list_path(err_file)?,
        None => ConsoleProgressInfo::new(),
    };
    let syncer = Syncer::new(
        &opt.source,
        &opt.destination,
        options.clone(),
        Box::new(console_info),
    );
    syncer.sync()
}

fn format_time(time: DateTime<Local>) -> String {
    time.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn watch_loop(opt: &Opt, options: &SyncOptions, interval: Duration) -> Result<(), Error> {
    loop {
        if let Err(err) = sync_once(opt, options) {
            eprintln!("{}", err);
        }
        let finished_at = Local::now();
        let next_at = finished_at
            .checked_add_signed(chrono::TimeDelta::seconds(interval.as_secs() as i64))
            .ok_or_else(|| anyhow::anyhow!("could not compute the time of the next sync"))?;
        let finished_str = format_time(finished_at);
        let next_str = format_time(next_at);
        announce_next_sync(&finished_str, &next_str);
        sleep_with_progress(interval, &next_str);
    }
}

fn main() -> Result<(), Error> {
    let opt = Opt::parse();
    let source = &opt.source;
    if !source.is_dir() {
        eprintln!("{} is not a directory", source.to_string_lossy());
        process::exit(1);
    }
    let options = make_options(&opt)?;
    match &opt.sleep_interval {
        None => match sync_once(&opt, &options) {
            Ok(stats) if stats.errors > 0 => process::exit(1),
            Ok(_) => process::exit(0),
            Err(err) => {
                eprintln!("{}", err);
                process::exit(1);
            }
        },
        Some(spec) => {
            let interval = parse_duration(spec)
                .with_context(|| format!("invalid --sleep-interval value '{}'", spec))?;
            watch_loop(&opt, &options, interval)?;
        }
    }
    Ok(())
}

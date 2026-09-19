use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;
use std::thread;
use std::time::Duration;

use tempfile::TempDir;

fn rusync(args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rusync"));
    cmd.args(args);
    cmd.output().expect("could not run rusync binary")
}

fn make_tree(tmp_path: &Path) -> (PathBuf, PathBuf) {
    let src_path = tmp_path.join("src");
    fs::create_dir_all(src_path.join("docs")).expect("could not create src");
    fs::write(src_path.join("docs/a.txt"), "a").expect("could not write a.txt");
    fs::write(src_path.join("docs/b.log"), "b").expect("could not write b.log");
    fs::write(src_path.join("docs/notes.tmp"), "n").expect("could not write notes.tmp");
    fs::write(src_path.join("plain.txt"), "p").expect("could not write plain.txt");
    let dest_path = tmp_path.join("dest");
    (src_path, dest_path)
}

fn run(tmp_path: &Path, args: &[&str]) -> Output {
    let (src_path, dest_path) = make_tree(tmp_path);
    let mut all_args: Vec<&str> = args.to_vec();
    all_args.push(src_path.to_str().expect("src path is not utf-8"));
    all_args.push(dest_path.to_str().expect("dest path is not utf-8"));
    let output = rusync(&all_args);
    assert!(
        output.status.success(),
        "rusync failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn invalid_exclude_regex_is_rejected_before_syncing() {
    let tmp_dir = TempDir::new().unwrap();
    let (src_path, dest_path) = make_tree(tmp_dir.path());
    let output = rusync(&[
        "--exclude",
        "foo(",
        src_path.to_str().unwrap(),
        dest_path.to_str().unwrap(),
    ]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--exclude"), "stderr was: {}", stderr);
    assert!(stderr.contains("foo("), "stderr was: {}", stderr);
    // The pattern is compiled before any file is written:
    assert!(!dest_path.exists());
}

#[test]
fn invalid_include_regex_is_rejected_before_syncing() {
    let tmp_dir = TempDir::new().unwrap();
    let (src_path, dest_path) = make_tree(tmp_dir.path());
    let output = rusync(&[
        "--include",
        "[",
        src_path.to_str().unwrap(),
        dest_path.to_str().unwrap(),
    ]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--include"), "stderr was: {}", stderr);
    assert!(!dest_path.exists());
}

#[test]
fn exclude_flag_filters_files() {
    let tmp_dir = TempDir::new().unwrap();
    let output = run(tmp_dir.path(), &["--exclude", r"\.log$"]);
    let dest = tmp_dir.path().join("dest");
    assert!(dest.join("docs/a.txt").exists());
    assert!(dest.join("docs/notes.tmp").exists());
    assert!(dest.join("plain.txt").exists());
    assert!(!dest.join("docs/b.log").exists());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("skipped by filters"),
        "stdout was: {}",
        stdout
    );
}

#[test]
fn include_flag_whitelists_paths() {
    let tmp_dir = TempDir::new().unwrap();
    run(tmp_dir.path(), &["--include", r"^docs/"]);
    let dest = tmp_dir.path().join("dest");
    assert!(dest.join("docs/a.txt").exists());
    assert!(dest.join("docs/b.log").exists());
    assert!(!dest.join("plain.txt").exists());
}

#[test]
fn filter_flags_can_be_repeated() {
    let tmp_dir = TempDir::new().unwrap();
    run(
        tmp_dir.path(),
        &["--exclude", r"\.log$", "--exclude", r"\.tmp$"],
    );
    let dest = tmp_dir.path().join("dest");
    assert!(dest.join("docs/a.txt").exists());
    assert!(dest.join("plain.txt").exists());
    assert!(!dest.join("docs/b.log").exists());
    assert!(!dest.join("docs/notes.tmp").exists());
}

#[test]
fn include_and_exclude_combine_on_the_command_line() {
    let tmp_dir = TempDir::new().unwrap();
    run(
        tmp_dir.path(),
        &["--include", r"^docs/", "--exclude", r"\.log$"],
    );
    let dest = tmp_dir.path().join("dest");
    assert!(dest.join("docs/a.txt").exists());
    assert!(dest.join("docs/notes.tmp").exists());
    assert!(!dest.join("docs/b.log").exists());
    assert!(!dest.join("plain.txt").exists());
}

#[test]
fn invalid_sleep_interval_is_rejected() {
    for bad in ["abc", "0", "1x", "1.5h", "10 m", ""] {
        let tmp_dir = TempDir::new().unwrap();
        let (src_path, dest_path) = make_tree(tmp_dir.path());
        let output = rusync(&[
            "--sleep-interval",
            bad,
            src_path.to_str().unwrap(),
            dest_path.to_str().unwrap(),
        ]);
        assert!(!output.status.success(), "'{}' was accepted", bad);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("--sleep-interval"),
            "stderr for '{}' was: {}",
            bad,
            stderr
        );
        // The interval is parsed before anything is written:
        assert!(!dest_path.exists());
    }
}

#[test]
fn help_documents_the_new_features() {
    for args in [vec!["--help"], vec!["-h"]] {
        let output = rusync(&args);
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        for needle in [
            "Minimalist rsync clone in Rust",
            "Source directory",
            "Destination directory",
            "--include <REGEX>",
            "--exclude <REGEX>",
            "--sleep-interval <INTERVAL>",
            "PATH MATCHING",
            "relative to SOURCE",
            "wins over",
            "not descended into",
            "WATCH MODE",
            "1h30m",
            "Ctrl-C",
        ] {
            assert!(
                stdout.contains(needle),
                "help ({}) is missing '{}', was:\n{}",
                args[0],
                needle,
                stdout
            );
        }
    }
}

#[test]
fn rusync2_is_also_installed_as_a_command() {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rusync2"));
    let output = cmd
        .arg("--help")
        .output()
        .expect("could not run the rusync2 binary");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Minimalist rsync clone in Rust"),
        "stdout was: {}",
        stdout
    );
    assert!(stdout.contains("PATH MATCHING"), "stdout was: {}", stdout);
}

#[test]
fn progress_is_shown_at_every_stage() {
    let tmp_dir = TempDir::new().unwrap();
    let (src_path, dest_path) = make_tree(tmp_dir.path());
    let src_str = src_path.to_str().unwrap();
    let dest_str = dest_path.to_str().unwrap();
    // First run: header, scanning line and summary are all displayed:
    let output = rusync(&[src_str, dest_str]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Syncing from"),
        "start header missing, stdout was: {}",
        stdout
    );
    assert!(
        stdout.contains("scanning"),
        "scanning line missing, stdout was: {}",
        stdout
    );
    assert!(
        stdout.contains("up to date"),
        "summary missing, stdout was: {}",
        stdout
    );
    assert!(
        stdout.contains("B/s"),
        "speed display missing, stdout was: {}",
        stdout
    );
    assert!(
        stdout.contains("avg "),
        "average speed display missing, stdout was: {}",
        stdout
    );
    assert!(
        stdout.contains("elapsed "),
        "elapsed time display missing, stdout was: {}",
        stdout
    );
    assert!(
        stdout.contains("eta "),
        "eta label missing, stdout was: {}",
        stdout
    );
    assert!(
        stdout.contains("(avg"),
        "average speed missing from the summary, stdout was: {}",
        stdout
    );
    // Second run, everything up to date: the header, the scanning line
    // and the per-file progress are still displayed:
    let output = rusync(&[src_str, dest_str]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Syncing from"), "stdout was: {}", stdout);
    assert!(stdout.contains("scanning"), "stdout was: {}", stdout);
    assert!(
        stdout.contains("0% 1/"),
        "per-file progress missing, stdout was: {}",
        stdout
    );
    assert!(
        stdout.contains("4 up to date"),
        "summary missing, stdout was: {}",
        stdout
    );
}

fn spawn_watch(src: &Path, dest: &Path, interval: &str) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_rusync"))
        .arg("--sleep-interval")
        .arg(interval)
        .arg(src)
        .arg(dest)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("could not start rusync")
}

#[test]
fn sleep_interval_repeats_syncs_until_killed() {
    let tmp_dir = TempDir::new().unwrap();
    let (src_path, dest_path) = make_tree(tmp_dir.path());
    let mut child = spawn_watch(&src_path, &dest_path, "1s");
    // Two syncs complete within the first ~2 seconds; leave slack for CI:
    thread::sleep(Duration::from_secs(3));
    child.kill().expect("could not kill rusync");
    let output = child.wait_with_output().expect("could not wait for rusync");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let rounds = stdout.matches("Synced").count();
    assert!(
        rounds >= 2,
        "only {} sync rounds ran; stdout was: {}",
        rounds,
        stdout
    );
    assert!(stdout.contains("next sync at"), "stdout was: {}", stdout);
    assert!(stdout.contains("waiting ["), "stdout was: {}", stdout);
    assert!(dest_path.join("plain.txt").exists());
}

#[test]
fn sleep_interval_accepts_human_suffixes() {
    for interval in ["1h", "10m", "1h30m"] {
        let tmp_dir = TempDir::new().unwrap();
        let (src_path, dest_path) = make_tree(tmp_dir.path());
        let mut child = spawn_watch(&src_path, &dest_path, interval);
        // The first sync runs immediately, then the sleep bar starts:
        thread::sleep(Duration::from_secs(1));
        child.kill().expect("could not kill rusync");
        let output = child.wait_with_output().expect("could not wait for rusync");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            dest_path.join("plain.txt").exists(),
            "nothing synced for interval '{}'",
            interval
        );
        assert!(
            stdout.contains("next sync at"),
            "stdout for '{}' was: {}",
            interval,
            stdout
        );
        assert!(
            stdout.contains("waiting ["),
            "stdout for '{}' was: {}",
            interval,
            stdout
        );
    }
}

#[test]
fn watch_mode_resyncs_changes() {
    let tmp_dir = TempDir::new().unwrap();
    let (src_path, dest_path) = make_tree(tmp_dir.path());
    let mut child = spawn_watch(&src_path, &dest_path, "1s");
    // Wait for the first sync to complete, then change a file and wait for
    // a later round to pick the change up:
    thread::sleep(Duration::from_millis(1200));
    fs::write(src_path.join("plain.txt"), "changed").unwrap();
    thread::sleep(Duration::from_millis(1800));
    child.kill().expect("could not kill rusync");
    child.wait_with_output().expect("could not wait for rusync");
    let synced = fs::read_to_string(dest_path.join("plain.txt")).unwrap();
    assert_eq!(synced, "changed");
}

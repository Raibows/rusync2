use std::fs;
use std::path::Path;
use std::path::PathBuf;

use tempfile::TempDir;

use rusync::progress::ProgressInfo;

struct DummyProgressInfo {}
impl ProgressInfo for DummyProgressInfo {}

fn to_owned_patterns(patterns: &[&str]) -> Vec<String> {
    patterns.iter().map(|p| p.to_string()).collect()
}

fn make_filters(include: &[&str], exclude: &[&str]) -> rusync::Filters {
    rusync::Filters::new(&to_owned_patterns(include), &to_owned_patterns(exclude))
        .expect("valid patterns")
}

/// Creates this source tree (10 files):
///
/// ```text
/// src/
///   build.sh
///   keep.log
///   plain.txt
///   build/one.out
///   docs/a.txt
///   docs/b.log
///   docs/notes.tmp
///   only_logs/x.log
///   sub/target/inner.out
///   target/build.out
/// ```
fn make_tree(tmp_path: &Path) -> (PathBuf, PathBuf) {
    let src_path = tmp_path.join("src");
    for dir in ["docs", "target", "sub/target", "only_logs", "build"] {
        fs::create_dir_all(src_path.join(dir)).expect("could not create source dir");
    }
    let files = [
        ("docs/a.txt", "a"),
        ("docs/b.log", "b"),
        ("docs/notes.tmp", "n"),
        ("target/build.out", "o1"),
        ("sub/target/inner.out", "o2"),
        ("build/one.out", "o3"),
        ("only_logs/x.log", "x"),
        ("keep.log", "k"),
        ("plain.txt", "p"),
        ("build.sh", "s"),
    ];
    for (name, contents) in files {
        let path = src_path.join(name);
        fs::write(path, contents).expect("could not write source file");
    }
    let dest_path = tmp_path.join("dest");
    (src_path, dest_path)
}

fn sync(tmp_path: &Path, include: &[&str], exclude: &[&str]) -> rusync::Stats {
    let (src_path, dest_path) = make_tree(tmp_path);
    let filters = make_filters(include, exclude);
    let options = rusync::SyncOptions {
        filters,
        ..Default::default()
    };
    let syncer = rusync::Syncer::new(
        &src_path,
        &dest_path,
        options,
        Box::new(DummyProgressInfo {}),
    );
    syncer.sync().expect("sync failed")
}

fn exists(dest: &Path, rel_path: &str) -> bool {
    dest.join(rel_path).exists()
}

/// All file paths below `root`, relative to it, sorted.
fn list_files(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if !root.exists() {
        return out;
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("could not read dest dir") {
            let entry = entry.expect("could not read dest entry");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path.strip_prefix(root).expect("dest entry under dest root");
                out.push(rel.to_string_lossy().to_string());
            }
        }
    }
    out.sort();
    out
}

#[test]
fn no_filters_syncs_everything() {
    let tmp_dir = TempDir::new().unwrap();
    let stats = sync(tmp_dir.path(), &[], &[]);
    assert_eq!(stats.num_files, 10);
    assert_eq!(stats.excluded_files, 0);
    assert_eq!(stats.excluded_dirs, 0);
    let dest = tmp_dir.path().join("dest");
    let mut expected: Vec<String> = vec![
        "docs/a.txt",
        "docs/b.log",
        "docs/notes.tmp",
        "target/build.out",
        "sub/target/inner.out",
        "build/one.out",
        "only_logs/x.log",
        "keep.log",
        "plain.txt",
        "build.sh",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    expected.sort();
    assert_eq!(list_files(&dest), expected);
}

#[test]
fn exclude_extension() {
    let tmp_dir = TempDir::new().unwrap();
    let stats = sync(tmp_dir.path(), &[], &[r"\.log$"]);
    assert_eq!(stats.num_files, 7);
    assert_eq!(stats.excluded_files, 3);
    assert_eq!(stats.excluded_dirs, 0);
    let dest = tmp_dir.path().join("dest");
    assert!(exists(&dest, "docs/a.txt"));
    assert!(exists(&dest, "docs/notes.tmp"));
    assert!(exists(&dest, "plain.txt"));
    assert!(exists(&dest, "build.sh"));
    assert!(exists(&dest, "target/build.out"));
    assert!(exists(&dest, "sub/target/inner.out"));
    assert!(exists(&dest, "build/one.out"));
    assert!(!exists(&dest, "docs/b.log"));
    assert!(!exists(&dest, "keep.log"));
    assert!(!exists(&dest, "only_logs/x.log"));
    // A directory containing only excluded files is not created:
    assert!(!exists(&dest, "only_logs"));
}

#[test]
fn prune_directory_by_name() {
    let tmp_dir = TempDir::new().unwrap();
    let stats = sync(tmp_dir.path(), &[], &[r"(^|/)target$"]);
    // Two directories match: `target` and `sub/target`
    assert_eq!(stats.excluded_dirs, 2);
    assert_eq!(stats.excluded_files, 0);
    // The two files inside pruned directories are never seen:
    assert_eq!(stats.num_files, 8);
    let dest = tmp_dir.path().join("dest");
    assert!(!exists(&dest, "target/build.out"));
    assert!(!exists(&dest, "sub/target/inner.out"));
    assert!(!exists(&dest, "target"));
    // `sub` only contained `target`, so it is not created either:
    assert!(!exists(&dest, "sub"));
    assert!(exists(&dest, "docs/a.txt"));
    assert!(exists(&dest, "keep.log"));
    assert!(exists(&dest, "build/one.out"));
    assert!(exists(&dest, "build.sh"));
    assert!(exists(&dest, "only_logs/x.log"));
}

#[test]
fn include_whitelist() {
    let tmp_dir = TempDir::new().unwrap();
    let stats = sync(tmp_dir.path(), &[r"^docs/"], &[]);
    assert_eq!(stats.num_files, 3);
    assert_eq!(stats.excluded_files, 7);
    assert_eq!(stats.excluded_dirs, 0);
    let dest = tmp_dir.path().join("dest");
    assert_eq!(
        list_files(&dest),
        vec!["docs/a.txt", "docs/b.log", "docs/notes.tmp"]
    );
}

#[test]
fn include_and_exclude_combine() {
    let tmp_dir = TempDir::new().unwrap();
    let stats = sync(tmp_dir.path(), &[r"^docs/"], &[r"\.log$"]);
    assert_eq!(stats.num_files, 2);
    assert_eq!(stats.excluded_files, 8);
    let dest = tmp_dir.path().join("dest");
    assert_eq!(list_files(&dest), vec!["docs/a.txt", "docs/notes.tmp"]);
}

#[test]
fn exclude_takes_precedence_over_include() {
    let tmp_dir = TempDir::new().unwrap();
    let stats = sync(tmp_dir.path(), &[r"\.log$"], &[r"^docs/"]);
    assert_eq!(stats.num_files, 2);
    let dest = tmp_dir.path().join("dest");
    assert_eq!(list_files(&dest), vec!["keep.log", "only_logs/x.log"]);
}

#[test]
fn directory_pattern_does_not_match_similar_file() {
    let tmp_dir = TempDir::new().unwrap();
    let stats = sync(tmp_dir.path(), &[], &[r"(^|/)build$"]);
    assert_eq!(stats.excluded_dirs, 1);
    assert_eq!(stats.num_files, 9);
    let dest = tmp_dir.path().join("dest");
    assert!(exists(&dest, "build.sh"));
    assert!(!exists(&dest, "build/one.out"));
    assert!(!exists(&dest, "build"));
}

#[test]
fn pattern_order_across_syncs_is_irrelevant() {
    let tmp_a = TempDir::new().unwrap();
    let stats_a = sync(
        tmp_a.path(),
        &[r"^docs/", r"\.tmp$"],
        &[r"\.log$", r"\.out$"],
    );
    let tmp_b = TempDir::new().unwrap();
    let stats_b = sync(
        tmp_b.path(),
        &[r"\.tmp$", r"^docs/"],
        &[r"\.out$", r"\.log$"],
    );
    assert_eq!(stats_a.num_files, stats_b.num_files);
    assert_eq!(stats_a.excluded_files, stats_b.excluded_files);
    assert_eq!(
        list_files(&tmp_a.path().join("dest")),
        list_files(&tmp_b.path().join("dest"))
    );
}

#[test]
fn case_sensitive_by_default() {
    let tmp_dir = TempDir::new().unwrap();
    let src_path = tmp_dir.path().join("src");
    fs::create_dir_all(&src_path).unwrap();
    fs::write(src_path.join("a.JPG"), "1").unwrap();
    fs::write(src_path.join("b.jpg"), "2").unwrap();
    let dest_path = tmp_dir.path().join("dest");
    let filters = make_filters(&[], &[r"\.JPG$"]);
    let options = rusync::SyncOptions {
        filters,
        ..Default::default()
    };
    let syncer = rusync::Syncer::new(
        &src_path,
        &dest_path,
        options,
        Box::new(DummyProgressInfo {}),
    );
    syncer.sync().unwrap();
    assert_eq!(list_files(&dest_path), vec!["b.jpg"]);
}

#[test]
fn inline_case_insensitive_flag() {
    let tmp_dir = TempDir::new().unwrap();
    let src_path = tmp_dir.path().join("src");
    fs::create_dir_all(&src_path).unwrap();
    fs::write(src_path.join("a.JPG"), "1").unwrap();
    fs::write(src_path.join("b.jpg"), "2").unwrap();
    let dest_path = tmp_dir.path().join("dest");
    let filters = make_filters(&[], &[r"(?i)\.jpg$"]);
    let options = rusync::SyncOptions {
        filters,
        ..Default::default()
    };
    let syncer = rusync::Syncer::new(
        &src_path,
        &dest_path,
        options,
        Box::new(DummyProgressInfo {}),
    );
    let stats = syncer.sync().unwrap();
    assert_eq!(stats.num_files, 0);
    assert_eq!(stats.excluded_files, 2);
    assert!(list_files(&dest_path).is_empty());
}

#[test]
fn unicode_paths_can_be_filtered() {
    let tmp_dir = TempDir::new().unwrap();
    let src_path = tmp_dir.path().join("src");
    fs::create_dir_all(src_path.join("中文")).unwrap();
    fs::write(src_path.join("中文/a.txt"), "a").unwrap();
    fs::create_dir_all(src_path.join("en")).unwrap();
    fs::write(src_path.join("en/b.txt"), "b").unwrap();
    let dest_path = tmp_dir.path().join("dest");
    let filters = make_filters(&[r"^中文/"], &[]);
    let options = rusync::SyncOptions {
        filters,
        ..Default::default()
    };
    let syncer = rusync::Syncer::new(
        &src_path,
        &dest_path,
        options,
        Box::new(DummyProgressInfo {}),
    );
    syncer.sync().unwrap();
    assert_eq!(list_files(&dest_path), vec!["中文/a.txt"]);
}

#[cfg(unix)]
#[test]
fn symlink_to_directory_can_be_pruned() {
    let tmp_dir = TempDir::new().unwrap();
    let src_path = tmp_dir.path().join("src");
    fs::create_dir_all(src_path.join("real")).unwrap();
    fs::write(src_path.join("real/inner.txt"), "i").unwrap();
    // Without a filter, rusync follows symlinks to directories; excluding
    // the link prunes it before it is descended into:
    std::os::unix::fs::symlink("real", src_path.join("link")).unwrap();
    let dest_path = tmp_dir.path().join("dest");
    let filters = make_filters(&[], &[r"(^|/)link$"]);
    let options = rusync::SyncOptions {
        filters,
        ..Default::default()
    };
    let syncer = rusync::Syncer::new(
        &src_path,
        &dest_path,
        options,
        Box::new(DummyProgressInfo {}),
    );
    let stats = syncer.sync().unwrap();
    assert_eq!(stats.excluded_dirs, 1);
    assert_eq!(stats.num_files, 1);
    assert!(exists(&dest_path, "real/inner.txt"));
    assert!(!exists(&dest_path, "link"));
    assert!(!exists(&dest_path, "link/inner.txt"));
}

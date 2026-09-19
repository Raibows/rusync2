//! filters
//!
//! Include and exclude rules based on regular expressions.

use anyhow::{Context, Error};
use regex::RegexSet;
use std::path::Path;

/// Include and exclude rules for a sync.
///
/// Patterns are regular expressions, matched against the path relative to
/// the source directory, with path components joined by `/` on every
/// platform.
///
/// Evaluation rules:
///
/// * If the include list is not empty, a path must match at least one
///   include pattern to be a candidate.
/// * A candidate matching any exclude pattern is excluded.
/// * `--exclude` takes precedence over `--include`.
/// * A directory matching any exclude pattern is pruned: `rusync` does not
///   descend into it. Include patterns never prevent descent, because
///   files below a directory may still match them.
#[derive(Debug, Clone)]
pub struct Filters {
    includes: Option<RegexSet>,
    excludes: RegexSet,
}

impl Filters {
    /// Build filters from the `--include` and `--exclude` command line
    /// arguments. Returns an error naming the first invalid pattern.
    pub fn new(include: &[String], exclude: &[String]) -> Result<Filters, Error> {
        let includes = if include.is_empty() {
            None
        } else {
            Some(build_set("include", include)?)
        };
        let excludes = build_set("exclude", exclude)?;
        Ok(Filters { includes, excludes })
    }

    /// Whether a file or symlink entry should be synchronized.
    pub fn passes(&self, rel_path: &Path) -> bool {
        let path = match_string(rel_path);
        let included = match &self.includes {
            Some(set) => set.is_match(&path),
            None => true,
        };
        included && !self.excludes.is_match(&path)
    }

    /// Whether a directory should be pruned (not descended into).
    ///
    /// Only exclude patterns apply here; include patterns never prevent
    /// descent, because files below the directory may still match them.
    pub fn dir_pruned(&self, rel_path: &Path) -> bool {
        self.excludes.is_match(&match_string(rel_path))
    }
}

impl Default for Filters {
    fn default() -> Self {
        Filters {
            includes: None,
            excludes: RegexSet::empty(),
        }
    }
}

/// The string a pattern is matched against: the relative path, with all
/// components joined by `/`, so that patterns behave the same on every
/// platform.
pub fn match_string(rel_path: &Path) -> String {
    rel_path
        .iter()
        .map(|component| component.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn build_set(kind: &str, patterns: &[String]) -> Result<RegexSet, Error> {
    for pattern in patterns {
        // Compile each pattern on its own, so that the error message can
        // name the offending pattern.
        regex::Regex::new(pattern)
            .with_context(|| format!("invalid --{} pattern: '{}'", kind, pattern))?;
    }
    // All patterns were validated above, so this cannot fail:
    let set = RegexSet::new(patterns)
        .with_context(|| format!("could not compile the --{} patterns", kind))?;
    Ok(set)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(include: &[&str], exclude: &[&str]) -> Filters {
        let include: Vec<String> = include.iter().map(|s| s.to_string()).collect();
        let exclude: Vec<String> = exclude.iter().map(|s| s.to_string()).collect();
        Filters::new(&include, &exclude).expect("valid patterns")
    }

    fn passes(filters: &Filters, rel_path: &str) -> bool {
        filters.passes(Path::new(rel_path))
    }

    fn pruned(filters: &Filters, rel_path: &str) -> bool {
        filters.dir_pruned(Path::new(rel_path))
    }

    #[test]
    fn empty_filters_pass_everything() {
        let filters = make(&[], &[]);
        assert!(passes(&filters, "a.txt"));
        assert!(passes(&filters, "docs/a.txt"));
        assert!(!pruned(&filters, "target"));
    }

    #[test]
    fn default_filters_pass_everything() {
        let filters = Filters::default();
        assert!(passes(&filters, "a/b.txt"));
        assert!(!pruned(&filters, "a"));
    }

    #[test]
    fn invalid_include_pattern_is_rejected() {
        let include = vec![String::from("foo(")];
        let err = Filters::new(&include, &[]).unwrap_err();
        let msg = format!("{:#}", err);
        assert!(msg.contains("--include"), "message was: {}", msg);
        assert!(msg.contains("foo("), "message was: {}", msg);
    }

    #[test]
    fn invalid_exclude_pattern_is_rejected() {
        let exclude = vec![String::from("[")];
        let err = Filters::new(&[], &exclude).unwrap_err();
        let msg = format!("{:#}", err);
        assert!(msg.contains("--exclude"), "message was: {}", msg);
        assert!(msg.contains('['), "message was: {}", msg);
    }

    #[test]
    fn error_names_the_first_invalid_pattern() {
        let include = vec![String::from("ok"), String::from("(")];
        let err = Filters::new(&include, &[]).unwrap_err();
        let msg = format!("{:#}", err);
        assert!(msg.contains("'('"), "message was: {}", msg);
    }

    #[test]
    fn include_whitelist() {
        let filters = make(&[r"^docs/"], &[]);
        assert!(passes(&filters, "docs/a.txt"));
        assert!(passes(&filters, "docs/sub/b.md"));
        assert!(!passes(&filters, "other/a.txt"));
    }

    #[test]
    fn include_anchors_are_user_supplied() {
        let filters = make(&[r"^docs/"], &[]);
        assert!(!passes(&filters, "docsx/a.txt"));
        assert!(!passes(&filters, "a/docs/x.txt"));
    }

    #[test]
    fn unanchored_include_matches_anywhere() {
        let filters = make(&["docs"], &[]);
        assert!(passes(&filters, "a/docs/b.txt"));
        assert!(passes(&filters, "docs/c.txt"));
        assert!(!passes(&filters, "other/x.txt"));
    }

    #[test]
    fn exclude_extension() {
        let filters = make(&[], &[r"\.log$"]);
        assert!(!passes(&filters, "a.log"));
        assert!(!passes(&filters, "a/b/c.log"));
        assert!(passes(&filters, "a.txt"));
        assert!(passes(&filters, "a.logg"));
        assert!(passes(&filters, "log"));
    }

    #[test]
    fn exclude_directory_pattern() {
        let filters = make(&[], &[r"(^|/)target$"]);
        assert!(pruned(&filters, "target"));
        assert!(pruned(&filters, "sub/target"));
        assert!(!pruned(&filters, "targetx"));
        assert!(!pruned(&filters, "target/inner.txt"));
        assert!(!pruned(&filters, "sub/targetx"));
        // A file below a pruned directory is never enumerated, but the
        // pattern for the directory itself does not match the file path:
        assert!(passes(&filters, "target/inner.txt"));
    }

    #[test]
    fn exclude_takes_precedence_over_include() {
        let filters = make(&[r"\.log$"], &[r"^docs/"]);
        assert!(!passes(&filters, "docs/a.log"));
        assert!(passes(&filters, "keep.log"));
        assert!(!passes(&filters, "docs/readme.txt"));
        assert!(!passes(&filters, "plain.txt"));
    }

    #[test]
    fn pattern_order_within_a_list_does_not_matter() {
        let a = make(&[r"^docs/", r"\.tmp$"], &[r"\.log$"]);
        let b = make(&[r"\.tmp$", r"^docs/"], &[r"\.log$"]);
        let samples = [
            "docs/a.txt",
            "docs/b.log",
            "docs/c.tmp",
            "other/d.tmp",
            "other/e.txt",
        ];
        for sample in samples {
            assert_eq!(passes(&a, sample), passes(&b, sample), "sample: {}", sample);
        }
    }

    #[test]
    fn empty_pattern_matches_everything() {
        let include_all = make(&[""], &[]);
        assert!(passes(&include_all, "anything/here.txt"));
        let exclude_all = make(&[], &[""]);
        assert!(!passes(&exclude_all, "anything/here.txt"));
        assert!(pruned(&exclude_all, "somedir"));
    }

    #[test]
    fn matching_is_case_sensitive() {
        let filters = make(&[], &[r"\.JPG$"]);
        assert!(!passes(&filters, "a.JPG"));
        assert!(passes(&filters, "a.jpg"));
    }

    #[test]
    fn inline_case_insensitive_flag_is_supported() {
        let filters = make(&[], &[r"(?i)\.jpg$"]);
        assert!(!passes(&filters, "a.JPG"));
        assert!(!passes(&filters, "b.jpg"));
    }

    #[test]
    fn unicode_paths_can_be_matched() {
        let filters = make(&[r"^中文/"], &[]);
        assert!(passes(&filters, "中文/a.txt"));
        assert!(!passes(&filters, "english/a.txt"));
        // `\w` matches non-ASCII word characters by default:
        let word = make(&[r"^\w+\.txt$"], &[]);
        assert!(passes(&word, "中文.txt"));
    }

    #[test]
    fn match_string_joins_components_with_forward_slash() {
        assert_eq!(match_string(Path::new("a.txt")), "a.txt");
        let joined = Path::new("a").join("b.log");
        assert_eq!(match_string(&joined), "a/b.log");
    }

    #[cfg(windows)]
    #[test]
    fn match_string_normalizes_windows_separators() {
        assert_eq!(match_string(Path::new(r"a\b.log")), "a/b.log");
        assert_eq!(match_string(Path::new(r"a\b\c.txt")), "a/b/c.txt");
    }

    #[test]
    fn regex_features_pass_through() {
        let alternation = make(&[r"^(docs|tools)/"], &[]);
        assert!(passes(&alternation, "docs/a"));
        assert!(passes(&alternation, "tools/b"));
        assert!(!passes(&alternation, "other/c"));

        let optional = make(&[], &[r"\.jpe?g$"]);
        assert!(!passes(&optional, "a.jpg"));
        assert!(!passes(&optional, "b.jpeg"));
        assert!(passes(&optional, "c.png"));

        let class = make(&[], &[r"^[a-c]\.txt$"]);
        assert!(!passes(&class, "b.txt"));
        assert!(passes(&class, "d.txt"));

        let repetition = make(&[], &[r"^x{2}\.txt$"]);
        assert!(!passes(&repetition, "xx.txt"));
        assert!(passes(&repetition, "xxx.txt"));
    }

    #[test]
    fn patterns_starting_with_a_dash_are_valid() {
        let filters = make(&[], &["-weird"]);
        assert!(!passes(&filters, "-weird"));
        assert!(passes(&filters, "normal.txt"));
    }
}

use std::fs;
use std::option::Option;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Entry {
    description: String,
    path: PathBuf,
    metadata: Option<fs::Metadata>,
    exists: bool,
    is_link: Option<bool>,
}

impl Entry {
    pub fn new(description: &str, entry_path: &Path) -> Entry {
        // A single `symlink_metadata` call is enough for regular files and
        // directories; only symlinks need a second call, to decide whether
        // they exist from the caller's point of view.
        let symlink_metadata = match fs::symlink_metadata(entry_path) {
            Ok(metadata) => metadata,
            Err(_) => {
                return Entry {
                    description: String::from(description),
                    metadata: None,
                    path: entry_path.to_path_buf(),
                    exists: false,
                    is_link: None,
                };
            }
        };
        let is_link = symlink_metadata.file_type().is_symlink();
        if is_link {
            Entry {
                description: String::from(description),
                metadata: Some(symlink_metadata),
                path: entry_path.to_path_buf(),
                // `exists` follows the link: it is false for broken symlinks:
                exists: entry_path.exists(),
                is_link: Some(true),
            }
        } else {
            Entry {
                description: String::from(description),
                metadata: Some(symlink_metadata),
                path: entry_path.to_path_buf(),
                exists: true,
                is_link: Some(false),
            }
        }
    }

    /// Build an entry for a regular file, reusing metadata that the walker
    /// already read, so that no extra syscall is needed.
    pub(crate) fn regular_file(
        description: &str,
        entry_path: &Path,
        metadata: fs::Metadata,
    ) -> Entry {
        Entry {
            description: String::from(description),
            metadata: Some(metadata),
            path: entry_path.to_path_buf(),
            exists: true,
            is_link: Some(false),
        }
    }

    pub fn description(&self) -> &String {
        &self.description
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }
    pub fn metadata(&self) -> Option<&fs::Metadata> {
        self.metadata.as_ref()
    }
    pub fn exists(&self) -> bool {
        self.exists
    }

    pub fn is_link(&self) -> Option<bool> {
        self.is_link
    }
}

#[cfg(test)]
mod tests {

    use super::Entry;
    use super::Path;

    #[test]
    fn new_entry_with_non_existing_path() {
        let path = Path::new("/path/to/nosuch.txt");
        let entry = Entry::new("nosuch", path);

        assert!(!entry.exists());
        assert!(entry.metadata.is_none());
    }

    #[test]
    fn new_entry_with_existing_path() {
        let path = Path::new(file!());
        let entry = Entry::new("entry.rs", path);

        assert!(entry.exists());
        assert!(entry.metadata.is_some());
        let is_link = entry.is_link();
        assert!(is_link.is_some());
        assert!(!is_link.unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn new_entry_with_regular_file_keeps_metadata() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let path = tmp_dir.path().join("file.txt");
        std::fs::write(&path, "contents").unwrap();
        let entry = Entry::new("file.txt", &path);
        assert!(entry.exists());
        assert_eq!(entry.metadata().unwrap().len(), 8);
        assert_eq!(entry.is_link(), Some(false));
    }

    #[cfg(unix)]
    #[test]
    fn new_entry_with_broken_symlink() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let path = tmp_dir.path().join("broken");
        std::os::unix::fs::symlink("no-such-file", &path).unwrap();
        let entry = Entry::new("broken", &path);
        // A broken symlink does not "exist" (following the link fails):
        assert!(!entry.exists());
        // ... but its metadata (that of the link itself) is available:
        assert!(entry.metadata().is_some());
        assert_eq!(entry.is_link(), Some(true));
    }

    #[cfg(unix)]
    #[test]
    fn new_entry_with_symlink_to_directory() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let dir = tmp_dir.path().join("dir");
        std::fs::create_dir(&dir).unwrap();
        let path = tmp_dir.path().join("link");
        std::os::unix::fs::symlink(&dir, &path).unwrap();
        let entry = Entry::new("link", &path);
        assert!(entry.exists());
        assert_eq!(entry.is_link(), Some(true));
    }
}

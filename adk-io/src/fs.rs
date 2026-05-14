use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, RwLock};

/// Filesystem boundary for core workflows.
///
/// This trait is intentionally generic/static-dispatch friendly: callers use
/// `Fs: FileSystem`, while the CLI uses `StdFileSystem` and tests or embedded
/// library callers can use `MemoryFileSystem`.
pub trait FileSystem {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        String::from_utf8(self.read(path)?)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    fn write(&self, path: &Path, contents: &[u8]) -> io::Result<()>;

    fn write_string(&self, path: &Path, contents: &str) -> io::Result<()> {
        self.write(path, contents.as_bytes())
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn remove_dir(&self, path: &Path) -> io::Result<()>;
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
    fn exists(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    fn is_file(&self, path: &Path) -> bool;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StdFileSystem;

impl FileSystem for StdFileSystem {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> io::Result<()> {
        std::fs::write(path, contents)
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }

    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_dir(path)
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let mut entries = std::fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<io::Result<Vec<_>>>()?;
        entries.sort();
        Ok(entries)
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }
}

#[derive(Clone, Debug)]
pub struct MemoryFileSystem {
    state: Arc<RwLock<MemoryState>>,
}

#[derive(Debug, Default)]
struct MemoryState {
    files: BTreeMap<PathBuf, Vec<u8>>,
    dirs: BTreeSet<PathBuf>,
}

impl Default for MemoryFileSystem {
    fn default() -> Self {
        let mut state = MemoryState::default();
        state.dirs.insert(PathBuf::new());
        Self {
            state: Arc::new(RwLock::new(state)),
        }
    }
}

impl MemoryFileSystem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn file_paths(&self) -> Vec<PathBuf> {
        self.state
            .read()
            .expect("memory filesystem poisoned")
            .files
            .keys()
            .cloned()
            .collect()
    }
}

impl FileSystem for MemoryFileSystem {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let path = normalize_path(path);
        self.state
            .read()
            .expect("memory filesystem poisoned")
            .files
            .get(&path)
            .cloned()
            .ok_or_else(|| not_found(path))
    }

    fn write(&self, path: &Path, contents: &[u8]) -> io::Result<()> {
        let path = normalize_path(path);
        let mut state = self.state.write().expect("memory filesystem poisoned");
        ensure_parent_dirs(&mut state, &path);
        state.dirs.remove(&path);
        state.files.insert(path, contents.to_vec());
        Ok(())
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        let path = normalize_path(path);
        let mut state = self.state.write().expect("memory filesystem poisoned");
        insert_dir_and_parents(&mut state, &path);
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        let path = normalize_path(path);
        let removed = self
            .state
            .write()
            .expect("memory filesystem poisoned")
            .files
            .remove(&path)
            .is_some();
        if removed {
            Ok(())
        } else {
            Err(not_found(path))
        }
    }

    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        let path = normalize_path(path);
        let mut state = self.state.write().expect("memory filesystem poisoned");
        if !state.dirs.contains(&path) {
            return Err(not_found(path));
        }
        if has_child(&state, &path) {
            return Err(io::Error::new(
                io::ErrorKind::DirectoryNotEmpty,
                format!("directory not empty: {}", path.display()),
            ));
        }
        state.dirs.remove(&path);
        Ok(())
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let path = normalize_path(path);
        let state = self.state.read().expect("memory filesystem poisoned");
        if !state.dirs.contains(&path) {
            return Err(not_found(path));
        }
        let mut entries = BTreeSet::new();
        for file in state.files.keys() {
            if parent_path(file) == Some(path.as_path()) {
                entries.insert(file.clone());
            }
        }
        for dir in &state.dirs {
            if dir != &path && parent_path(dir) == Some(path.as_path()) {
                entries.insert(dir.clone());
            }
        }
        Ok(entries.into_iter().collect())
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        let path = normalize_path(path);
        if self.exists(&path) {
            Ok(path)
        } else {
            Err(not_found(path))
        }
    }

    fn exists(&self, path: &Path) -> bool {
        let path = normalize_path(path);
        let state = self.state.read().expect("memory filesystem poisoned");
        state.files.contains_key(&path) || state.dirs.contains(&path)
    }

    fn is_dir(&self, path: &Path) -> bool {
        let path = normalize_path(path);
        self.state
            .read()
            .expect("memory filesystem poisoned")
            .dirs
            .contains(&path)
    }

    fn is_file(&self, path: &Path) -> bool {
        let path = normalize_path(path);
        self.state
            .read()
            .expect("memory filesystem poisoned")
            .files
            .contains_key(&path)
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

fn ensure_parent_dirs(state: &mut MemoryState, path: &Path) {
    if let Some(parent) = parent_path(path) {
        insert_dir_and_parents(state, parent);
    }
}

fn insert_dir_and_parents(state: &mut MemoryState, path: &Path) {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        state.dirs.insert(current.clone());
    }
    if path.as_os_str().is_empty() {
        state.dirs.insert(PathBuf::new());
    }
}

fn parent_path(path: &Path) -> Option<&Path> {
    path.parent().filter(|parent| parent != &Path::new(""))
}

fn has_child(state: &MemoryState, path: &Path) -> bool {
    state
        .files
        .keys()
        .any(|file| parent_path(file) == Some(path))
        || state
            .dirs
            .iter()
            .any(|dir| dir != path && parent_path(dir) == Some(path))
}

fn not_found(path: PathBuf) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("path not found: {}", path.display()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_filesystem_roundtrips_files_and_directories() {
        let fs = MemoryFileSystem::new();
        fs.write_string(
            Path::new("project/topics/greeting.yaml"),
            "name: greeting\n",
        )
        .expect("write file");

        assert!(fs.exists(Path::new("project/topics")));
        assert!(fs.is_dir(Path::new("")));
        assert!(fs.is_dir(Path::new("project/topics")));
        assert!(fs.is_file(Path::new("project/topics/greeting.yaml")));
        assert_eq!(
            fs.read_to_string(Path::new("project/topics/greeting.yaml"))
                .expect("read file"),
            "name: greeting\n"
        );

        let entries = fs.read_dir(Path::new("project/topics")).expect("read dir");
        assert_eq!(entries, vec![PathBuf::from("project/topics/greeting.yaml")]);
    }

    #[test]
    fn memory_filesystem_removes_empty_directories_only() {
        let fs = MemoryFileSystem::new();
        fs.write_string(
            Path::new("project/topics/greeting.yaml"),
            "name: greeting\n",
        )
        .expect("write file");

        assert_eq!(
            fs.remove_dir(Path::new("project/topics"))
                .expect_err("non-empty")
                .kind(),
            io::ErrorKind::DirectoryNotEmpty
        );

        fs.remove_file(Path::new("project/topics/greeting.yaml"))
            .expect("remove file");
        fs.remove_dir(Path::new("project/topics"))
            .expect("remove empty dir");
        assert!(!fs.exists(Path::new("project/topics")));
    }
}

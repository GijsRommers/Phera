//! A fresh project per CLI test. Only files created by the test are removed.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct Project(pub PathBuf);

impl Project {
    pub fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "phera-env-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }

    pub fn write(&self, file: &str, data: impl AsRef<[u8]>) -> PathBuf {
        let path = self.0.join(Path::new(file));
        std::fs::write(&path, data).unwrap();
        path
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

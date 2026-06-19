use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_secs()
}

pub(super) fn epoch_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos()
}

pub struct TestTempDir {
    pub path: PathBuf,
    remove_on_drop: bool,
}

impl TestTempDir {
    pub fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            epoch_seconds()
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        Self {
            path,
            remove_on_drop: false,
        }
    }

    pub fn remove_on_drop(mut self) {
        self.remove_on_drop = true;
    }
}

impl Drop for TestTempDir {
    fn drop(&mut self) {
        if self.remove_on_drop
            && std::env::var("HINEMOS_VERIFY_KEEP_LOGS").ok().as_deref() != Some("1")
        {
            fs::remove_dir_all(&self.path).ok();
        } else {
            eprintln!("verifier logs kept at {}", self.path.display());
        }
    }
}

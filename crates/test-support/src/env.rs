use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate should live under workspace/crates/test-support")
        .to_path_buf()
}

pub fn load_local_env(root: &Path) -> HashMap<String, String> {
    let mut values = HashMap::new();
    for env_path in [
        root.join(".env"),
        root.join(".env.test"),
        root.join(".env.local"),
    ] {
        let Ok(contents) = fs::read_to_string(env_path) else {
            continue;
        };
        for line in contents.lines() {
            if let Some((key, value)) = parse_env_line(line) {
                values.entry(key).or_insert(value);
            }
        }
    }

    for key in [
        "DATABASE_URL",
        "HERMES_TEST_PROVIDER",
        "HERMES_TEST_MODEL",
        "HERMES_TEST_BASE_URL",
        "HERMES_TEST_API_MODE",
    ] {
        if let Ok(value) = std::env::var(key) {
            values.insert(key.to_owned(), value);
        }
    }
    values
}

fn parse_env_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let line = line.strip_prefix("export ").unwrap_or(line).trim();
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    let value = value.trim();
    let value = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value);
    Some((key.to_owned(), value.to_owned()))
}

pub(crate) fn assert_database_env(values: &HashMap<String, String>) -> String {
    values
        .get("DATABASE_URL")
        .filter(|value| !value.is_empty())
        .cloned()
        .expect("DATABASE_URL is required in the shell, .env, or local-only .env.test")
}

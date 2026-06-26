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
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_MODEL",
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

pub fn assert_provider_env(values: &HashMap<String, String>) {
    let missing = [
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_MODEL",
    ]
    .into_iter()
    .filter(|key| values.get(*key).is_none_or(String::is_empty))
    .collect::<Vec<_>>();

    assert!(
        missing.is_empty(),
        "Claude provider environment is incomplete. Missing: {}. Set them in your shell or local-only .env.local.",
        missing.join(", ")
    );
}

pub fn assert_gpt_provider_env(values: &HashMap<String, String>) {
    assert_provider_env(values);

    let model = values
        .get("ANTHROPIC_MODEL")
        .expect("ANTHROPIC_MODEL was checked by assert_provider_env");
    assert!(
        model.to_ascii_lowercase().contains("gpt"),
        "LLM provider must use a GPT model through the rotom provider. Set ANTHROPIC_MODEL to a GPT-backed model; current model is `{model}`."
    );
}

pub(crate) fn assert_database_env(values: &HashMap<String, String>) -> String {
    values
        .get("DATABASE_URL")
        .filter(|value| !value.is_empty())
        .cloned()
        .expect("DATABASE_URL is required in the shell, .env, or local-only .env.test")
}

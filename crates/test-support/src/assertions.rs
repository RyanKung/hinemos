use crate::TestTempDir;

pub fn assert_contains(output: &str, needle: &str, description: &str) {
    assert!(
        output.contains(needle),
        "missing {description}: expected `{needle}` in\n{output}"
    );
}

pub fn assert_not_contains(output: &str, needle: &str, description: &str) {
    assert!(
        !output.contains(needle),
        "unexpected {description}: found `{needle}` in\n{output}"
    );
}

pub fn parse_hash_id(output: &str, prefix: &str) -> i64 {
    let start = output
        .find(prefix)
        .unwrap_or_else(|| panic!("missing id prefix `{prefix}` in\n{output}"))
        + prefix.len();
    let id = output[start..]
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    id.parse::<i64>()
        .unwrap_or_else(|error| panic!("invalid id after `{prefix}`: {error}\n{output}"))
}

pub fn require_output(stdout: &str, needles: &[&str], description: &str, temp: &TestTempDir) {
    let found = needles.iter().any(|needle| {
        stdout
            .to_ascii_lowercase()
            .contains(&needle.to_ascii_lowercase())
    });
    assert!(
        found,
        "Claude verifier output is missing: {description}\nlogs: {}\nstdout:\n{stdout}",
        temp.path.display()
    );
}

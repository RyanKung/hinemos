use super::*;

#[test]
fn memory_command_rest_matches_only_memory_namespace() {
    assert_eq!(memory_command_rest("/memory"), Some(""));
    assert_eq!(
        memory_command_rest("  /memory recall alice  "),
        Some("recall alice")
    );
    assert_eq!(memory_command_rest("/memoryx recall alice"), None);
    assert_eq!(memory_command_rest("/mem"), None);
}

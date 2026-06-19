mod assertions;
mod database;
mod env;
mod llm;
mod process;
mod server;
mod ssh;
mod temp;

pub use assertions::*;
pub use database::*;
pub use env::*;
pub use llm::*;
pub use process::{
    assert_command_exists, copy_dir_recursive, free_local_port, terminate, wait_with_timeout,
};
pub use server::*;
pub use ssh::*;
pub use temp::{TestTempDir, epoch_seconds};

#[test]
fn common_helpers_are_reachable_for_lints() {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Output};
    use std::time::Duration;

    let _ = workspace_root as fn() -> PathBuf;
    let _ = load_local_env as fn(&Path) -> HashMap<String, String>;
    let _ = assert_provider_env as fn(&HashMap<String, String>);
    let _ = assert_command_exists as fn(&str);
    let _ = free_local_port as fn() -> u16;
    let _ = spawn_hinemos_server as fn(&Path, &str, u16, &Path, &str) -> Child;
    let _ = spawn_hinemos_server_with_env::<0>
        as fn(&Path, &str, u16, &Path, &str, [(&str, &str); 0]) -> Child;
    let _ = spawn_hinemos_rooms as fn(&Path, &Path, &str, u64) -> Child;
    let _ = run_hinemos_rooms_once as fn(&Path, &str) -> String;
    let _ = wait_for_server as fn(&str, u16, &mut Child, &Path);
    let _ = run_ssh_batch::<0> as fn(&str, u16, &str, [&str; 0]) -> String;
    let _ = generate_ed25519_key as fn(&Path);
    let _ = run_ssh_batch_with_key as fn(&str, u16, &str, &Path, &[&str]) -> String;
    let _ = admit_ssh_user as fn(&str, u16, &str, &Path);
    let _ = admitted_key as fn(&TestTempDir, &str, u16, &str) -> PathBuf;
    let _ = SshSession::spawn as fn(&str, u16, &str) -> SshSession;
    let _ = SshSession::spawn_with_key as fn(&str, u16, &str, &Path) -> SshSession;
    let _ = SshSession::spawn_exec::<0> as fn(&str, u16, &str, [&str; 0]) -> SshSession;
    let _ =
        SshSession::spawn_exec_with_key::<0> as fn(&str, u16, &str, &Path, [&str; 0]) -> SshSession;
    let _ = SshSession::write_line as fn(&mut SshSession, &str);
    let _ = SshSession::wait_for_stdout as fn(&SshSession, &str, Duration);
    let _ = SshSession::wait_for_any_stdout as fn(&SshSession, &[&str], Duration) -> String;
    let _ = SshSession::wait_success as fn(SshSession, Duration) -> String;
    let _ = assert_contains as fn(&str, &str, &str);
    let _ = assert_not_contains as fn(&str, &str, &str);
    let _ = parse_hash_id as fn(&str, &str) -> i64;
    let _ = run_claude_agent as fn(&str, &HashMap<String, String>, Duration) -> AgentRun;
    let _ = run_claude_agent_until
        as fn(&str, &HashMap<String, String>, Duration, fn(&str) -> bool) -> AgentRun;
    let _ = wait_with_timeout as fn(Child, Duration) -> Output;
    let _ = terminate as fn(&mut Child);
    let _ = copy_dir_recursive as fn(&Path, &Path);
    let _ = require_output as fn(&str, &[&str], &str, &TestTempDir);
    let _ = epoch_seconds as fn() -> u64;
    let _ = TestTempDir::new as fn(&str) -> TestTempDir;
    let _ = TestTempDir::remove_on_drop as fn(TestTempDir);
    let _ = TestDatabase::create as fn(&HashMap<String, String>) -> TestDatabase;
    let _ = TestDatabase::query_value as fn(&TestDatabase, &str) -> String;

    let run = AgentRun {
        success: false,
        timed_out: false,
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    let _ = (
        run.success,
        run.timed_out,
        run.stdout.len(),
        run.stderr.len(),
    );
}

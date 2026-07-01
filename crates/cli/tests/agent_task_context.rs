mod common;

use common::*;

#[test]
fn admitted_ssh_user_receives_resident_context_and_self_model() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-agent-task-context");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("agentctx{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);
    let key = admitted_key(&temp, host, port, &user);

    let output =
        run_ssh_batch_with_key(host, port, &user, &key, &["/look", "/memory self", "/quit"]);
    assert_contains(
        &output,
        "Resident context:",
        "logged-in world observation includes resident context",
    );
    assert_contains(
        &output,
        "Use only visible Hinemos commands",
        "resident context stays inside the ordinary game command surface",
    );
    assert_contains(
        &output,
        "Memory: /memory self, /memory commitments.",
        "resident context points humans and agents to existing memory commands",
    );
    assert_contains(
        &output,
        "Social drives:",
        "resident context exposes live social and subjective meters",
    );
    assert_contains(
        &output,
        "Self memory",
        "memory command can read the persisted self-model",
    );
    assert_contains(
        &output,
        "taskObjective",
        "persisted self-model records the task objective",
    );
    assert_contains(
        &output,
        "\"lastStep\"",
        "memory command shows the latest evaluated task step",
    );
    assert_contains(
        &output,
        "\"commandLine\":\"/look\"",
        "task step is tied to the real world command that just ran",
    );

    let player_id = test_database.query_value(&format!(
        "select player_id from ssh_identities where username = '{user}'"
    ));
    let model_count = test_database.query_value(&format!(
        "select count(*)
         from agent_self_models
         where agent_id = '{player_id}'
           and identity->>'name' = '{user}'
           and identity->>'taskObjective' like 'As {user}, earn MARK%'"
    ));
    assert_ne!(
        model_count, "0",
        "logged-in resident context must be backed by a persisted self-model"
    );
    let latest_snapshot_view = test_database.query_value(&format!(
        "select current_state->'lastSnapshot'->>'viewId'
         from agent_self_models
         where agent_id = '{player_id}'
         order by version desc
         limit 1"
    ));
    assert_eq!(
        latest_snapshot_view, "arrival_street",
        "resident context refresh writes the latest visible world snapshot"
    );
    let live_meter_types = test_database.query_value(&format!(
        "select concat_ws(':',
             coalesce(jsonb_typeof(current_state->'lastSnapshot'->'socialContactUnits'), 'missing'),
             coalesce(jsonb_typeof(current_state->'lastSnapshot'->'standingUnits'), 'missing'),
             coalesce(jsonb_typeof(current_state->'lastSnapshot'->'commitmentSatisfactionUnits'), 'missing'),
             coalesce(jsonb_typeof(current_state->'lastSnapshot'->'lonelinessPoints'), 'missing'),
             coalesce(jsonb_typeof(current_state->'lastSnapshot'->'boredomPoints'), 'missing'))
         from agent_self_models
         where agent_id = '{player_id}'
         order by version desc
         limit 1"
    ));
    assert_eq!(
        live_meter_types, "number:number:number:number:number",
        "resident context persists live social and subjective meters"
    );
    let latest_step_shape = test_database.query_value(&format!(
        "select concat_ws(':',
             current_state->'lastStep'->>'commandLine',
             coalesce(jsonb_typeof(current_state->'lastStep'->'reward'), 'missing'),
             coalesce(jsonb_typeof(current_state->'lastStep'->'boredomReliefDelta'), 'missing'),
             coalesce(jsonb_typeof(current_state->'commandHistory'), 'missing'))
         from agent_self_models
         where agent_id = '{player_id}'
         order by version desc
         limit 1"
    ));
    assert_eq!(
        latest_step_shape, "/look:number:number:array",
        "resident task loop persists an evaluated command transition"
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

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

    let output = run_ssh_batch_with_key(
        host,
        port,
        &user,
        &key,
        &[
            "/go east",
            "/who",
            "/go east",
            "/parcel info E2-C0-01",
            "/parcel claim E2-C0-01",
            "/memory report I walked the east road and found no residents yet.",
            "/memory self",
            "/memory self",
            "/quit",
        ],
    );
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
        "Subject: In Hinemos, you are the logged-in resident",
        "resident context fixes the in-world subject to the authenticated player",
    );
    assert!(
        output
            .matches("Subject: In Hinemos, you are the logged-in resident")
            .count()
            >= 2,
        "subsequent observations should keep the resident subject boundary visible"
    );
    assert_contains(
        &output,
        "Autonomy: For ordinary safe in-world actions",
        "resident context requires autonomous choice for ordinary in-world actions",
    );
    assert!(
        output
            .matches("Autonomy: For ordinary safe in-world actions")
            .count()
            >= 2,
        "subsequent observations should keep the resident autonomy boundary visible"
    );
    assert_contains(
        &output,
        "Memory: /memory self, /memory commitments, /memory report <text>.",
        "resident context points humans and agents to existing memory commands",
    );
    assert_contains(
        &output,
        "Social drives:",
        "resident context exposes live social and subjective meters",
    );
    assert_contains(
        &output,
        "Virtual time: one in-world day is 300 real seconds",
        "resident context exposes the configured virtual day length",
    );
    assert_contains(
        &output,
        "Loop: day",
        "resident context exposes the current in-world loop status",
    );
    assert_contains(
        &output,
        "daily report due",
        "new residents are visibly prompted by in-world state to write the day's report",
    );
    assert_contains(
        &output,
        "daily report ready",
        "resident loop becomes report-ready only after in-world searching",
    );
    assert_contains(
        &output,
        "East 1 Rd.",
        "baseline resident path enters the generated east grid road",
    );
    assert_contains(
        &output,
        "East 2 Rd.",
        "baseline resident path can continue extending the generated grid",
    );
    assert_contains(
        &output,
        "E2-C0-01",
        "generated roads expose building doorplates instead of road-owned lots",
    );
    assert_contains(
        &output,
        "Parcel E2-C0-01",
        "dynamic doorplates can be inspected as virtual land",
    );
    assert_contains(
        &output,
        "Status: vacant",
        "dynamic doorplates can be inspected as virtual vacant land",
    );
    assert_contains(
        &output,
        "Claimed parcel E2-C0-01.",
        "dynamic doorplates can be claimed as a home site",
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
        "subjectBoundary",
        "memory output records the resident subject boundary",
    );
    assert_contains(
        &output,
        "Decide ordinary in-world next steps yourself",
        "memory output records the resident autonomy boundary",
    );
    assert_contains(
        &output,
        "\"lastStep\"",
        "memory command shows the latest evaluated task step",
    );
    assert_contains(
        &output,
        "\"commandLine\":\"/memory report I walked the east road and found no residents yet.\"",
        "memory output sees the visible daily report command that just ran before it",
    );
    assert_contains(
        &output,
        "Daily report recorded.",
        "resident can write a daily report through the visible memory surface",
    );
    assert_contains(
        &output,
        "daily report complete",
        "/memory self renders the completed loop state after a report",
    );
    assert_contains(
        &output,
        "\"reportDue\":false",
        "memory output exposes the completed virtual-day report state",
    );
    assert_not_contains(
        &output,
        "Sent to room service",
        "baseline resident path should not depend on service rooms",
    );

    let player_id = test_database.query_value(&format!(
        "select player_id from ssh_identities where username = '{user}'"
    ));
    let model_count = test_database.query_value(&format!(
        "select count(*)
         from agent_self_models
         where agent_id = '{player_id}'
           and identity->>'name' = '{user}'
           and identity->>'taskObjective' like 'As {user}, search the town%'"
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
        latest_snapshot_view, "grid_road_xp2_y0",
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
        latest_step_shape, "/memory self:number:number:array",
        "resident task loop persists an evaluated command transition"
    );
    let virtual_day_state = test_database.query_value(&format!(
        "select concat_ws(':',
             current_state->'virtualTime'->>'reportDue',
             coalesce(current_state->'virtualTime'->>'lastReportDay', 'missing'),
             coalesce(jsonb_typeof(current_state->'lastSnapshot'->'virtualDay'), 'missing'),
             current_state->'virtualTime'->>'reportReady',
             coalesce(current_state->'virtualTime'->>'searchesToday', 'missing'),
             coalesce(jsonb_typeof(current_state->'virtualTime'->'searchesToday'), 'missing'))
         from agent_self_models
         where agent_id = '{player_id}'
         order by version desc
         limit 1"
    ));
    let virtual_day_parts = virtual_day_state.split(':').collect::<Vec<_>>();
    assert_eq!(
        virtual_day_parts.len(),
        6,
        "virtual-day state should expose report due, last report day, snapshot day type, report readiness, and search count: {virtual_day_state}"
    );
    assert_eq!(
        virtual_day_parts[0], "false",
        "daily report command should close the current day's report loop"
    );
    assert_ne!(
        virtual_day_parts[1], "missing",
        "daily report command should stamp the current virtual day"
    );
    assert_eq!(
        virtual_day_parts[2], "number",
        "resident snapshots should be tied to the current virtual day"
    );
    assert_eq!(
        virtual_day_parts[3], "false",
        "completed daily report should clear report readiness"
    );
    let searches_today = virtual_day_parts[4].parse::<i64>().expect("searchesToday");
    assert!(
        searches_today > 0,
        "daily report completion should preserve evidence of same-day searching"
    );
    assert_eq!(
        virtual_day_parts[5], "number",
        "resident loop should persist the same-day search counter"
    );
    let report_step_metrics = test_database.query_value(&format!(
        "select concat_ws(':',
             current_state->'lastStep'->>'reward',
             current_state->'lastStep'->>'progressDelta',
             current_state->'lastStep'->>'lonelinessReliefDelta',
             current_state->'lastStep'->>'boredomReliefDelta',
             current_state->'virtualTime'->>'reportDue')
         from agent_self_models
         where agent_id = '{player_id}'
           and current_state->'lastStep'->>'commandLine' = '/memory report I walked the east road and found no residents yet.'
         order by version desc
         limit 1"
    ));
    let report_step_parts = report_step_metrics.split(':').collect::<Vec<_>>();
    assert_eq!(
        report_step_parts.len(),
        5,
        "report step should persist reward, progress, relief, and report status: {report_step_metrics}"
    );
    let report_reward = report_step_parts[0]
        .parse::<i64>()
        .expect("daily report reward");
    let report_progress_delta = report_step_parts[1]
        .parse::<i64>()
        .expect("daily report progress delta");
    let report_loneliness_relief = report_step_parts[2]
        .parse::<i64>()
        .expect("daily report loneliness relief");
    let report_boredom_relief = report_step_parts[3]
        .parse::<i64>()
        .expect("daily report boredom relief");
    assert!(
        report_reward > 0,
        "due daily report should receive positive loop reward, got {report_reward}"
    );
    assert!(
        report_progress_delta > 0,
        "due daily report should advance resident loop progress"
    );
    assert!(
        report_loneliness_relief >= 0,
        "daily report should not deepen loneliness pressure"
    );
    assert!(
        report_boredom_relief >= 0,
        "daily report should not deepen boredom pressure"
    );
    assert_eq!(
        report_step_parts[4], "false",
        "report step should mark the current virtual day complete"
    );
    let memory_self_step_count = test_database.query_value(&format!(
        "select count(*)
         from agent_self_models
         where agent_id = '{player_id}'
           and current_state->'lastStep'->>'commandLine' = '/memory self'"
    ));
    assert_eq!(
        memory_self_step_count, "2",
        "regression path must record repeated memory introspection commands"
    );
    let claimed_grid_parcel = test_database.query_value(&format!(
        "select concat_ws(':', parcel_id, front_view_id, owner_user)
         from parcels
         where parcel_id = 'E2-C0-01'
           and owner_user = '{user}'"
    ));
    assert_eq!(
        claimed_grid_parcel,
        format!("E2-C0-01:grid_road_xp2_y0:{user}"),
        "claiming a generated doorplate must materialize the parcel row"
    );
    let daily_report_emotion = test_database.query_value(&format!(
        "select concat_ws(':',
             object->'emotion'->>'status',
             coalesce(object->'emotion'->'primaryMood'->>'mood', 'missing'),
             coalesce(jsonb_typeof(object->'emotion'->'activeMoods'), 'missing'))
         from memory_atoms
         where agent_id = '{player_id}'
           and kind = 'self'
           and predicate = 'last_daily_report'
         limit 1"
    ));
    assert_ne!(
        daily_report_emotion, "scored:missing:array",
        "DADOES should provide a primary mood for the daily report"
    );
    assert!(
        daily_report_emotion.starts_with("scored:"),
        "daily report should be scored by DADOES, got {daily_report_emotion}"
    );
    let event_signature_chars = test_database.query_value(&format!(
        "select char_length(current_state->'lastSnapshot'->>'eventSignature')
         from agent_self_models
         where agent_id = '{player_id}'
         order by version desc
         limit 1"
    ));
    let event_signature_chars = event_signature_chars
        .parse::<usize>()
        .expect("event signature char length");
    assert!(
        event_signature_chars <= 560,
        "resident task event signature should stay bounded after repeated /memory self, got {event_signature_chars}"
    );
    let command_history = test_database.query_value(&format!(
        "select coalesce(string_agg(entry->>'commandLine', ','), '')
         from (
             select current_state
             from agent_self_models
             where agent_id = '{player_id}'
             order by version desc
             limit 1
         ) latest,
         jsonb_array_elements(latest.current_state->'commandHistory') entry"
    ));
    assert_contains(
        &command_history,
        "/memory report I walked the east road and found no residents yet.",
        "resident task history records the in-world daily report command",
    );
    assert_contains(
        &command_history,
        "/memory self",
        "resident task history records a visible app-view command",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

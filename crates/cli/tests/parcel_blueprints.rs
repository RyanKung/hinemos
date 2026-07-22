mod common;

use std::collections::BTreeSet;
use std::fs;

use common::*;
use hinemos_core::command::BuildSheet;
use serde_json::Value;

#[test]
fn newspaper_blueprint_covers_deployment_and_operational_validation() {
    let blueprint = newspaper_blueprint();
    assert_eq!(string_field(&blueprint, "kind"), "parcelBlueprint");
    assert!(
        number_field(&blueprint, "version") >= 8,
        "newspaper blueprint version should advance when job guides are added"
    );
    assert_eq!(string_field(&blueprint, "id"), "oracle-daily-newspaper");
    assert!(bool_field(
        value_field(&blueprint, "boundary"),
        "notBuiltinRoom"
    ));

    let build_sheet: BuildSheet =
        serde_json::from_value(value_field(&blueprint, "buildSheet").clone())
            .expect("newspaper buildSheet should parse as a generic parcel BuildSheet");
    let commands = build_sheet
        .commands
        .expect("newspaper buildSheet should define custom parcel commands");
    assert!(
        commands.contains("/paper submit")
            && commands.contains("/paper weekly unsubscribe")
            && commands.contains("/paper ledger summary")
            && commands.contains("approve|reject|revise"),
        "newspaper buildSheet commands should expose submission, opt-out, ledger, and revise surfaces"
    );

    assert_command_routes_target_existing_desks(&blueprint, &commands);
    assert_deployment_commands_cover_blueprint_entities(&blueprint);
    assert_job_guides_are_deployable(&blueprint);
    assert_roles_target_existing_desks_and_lists(&blueprint);
    assert_validation_tests_define_storyline_acceptance(&blueprint);
    assert_validation_tests_cover_workflow_states(&blueprint);
}

fn newspaper_blueprint() -> Value {
    let path = workspace_root().join("docs/parcel-blueprints/newspaper.json");
    let contents = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read newspaper blueprint {}: {error}", path.display()));
    serde_json::from_str(&contents)
        .unwrap_or_else(|error| panic!("parse newspaper blueprint {}: {error}", path.display()))
}

fn assert_command_routes_target_existing_desks(blueprint: &Value, commands: &str) {
    let desks = slug_set(array_field(&blueprint["workDesks"], "workDesks"), "slug");
    let command_routes = array_field(&blueprint["commandRoutes"], "commandRoutes");
    assert_eq!(
        command_routes.len(),
        9,
        "newspaper should route every /paper command family"
    );

    for route in command_routes {
        let prefix = string_field(route, "commandPrefix");
        let target = string_field(route, "targetDesk");
        assert!(
            desks.contains(target),
            "route {prefix} targets unknown desk {target}"
        );
        assert!(
            commands.contains(prefix),
            "buildSheet commands should advertise routed prefix {prefix}"
        );
    }
}

fn assert_deployment_commands_cover_blueprint_entities(blueprint: &Value) {
    let deployment = value_field(blueprint, "deploymentCommands");
    let work_desks = array_field(&blueprint["workDesks"], "workDesks");
    let communication_lists = array_field(&blueprint["communicationLists"], "communicationLists");
    let command_routes = array_field(&blueprint["commandRoutes"], "commandRoutes");
    let create_work_desks = string_array_field(deployment, "createWorkDesks");
    let create_lists = string_array_field(deployment, "createCommunicationLists");
    let create_routes = string_array_field(deployment, "createRoutes");

    assert_eq!(
        create_work_desks.len(),
        work_desks.len(),
        "deployment commands should create every work desk"
    );
    assert_eq!(
        create_lists.len(),
        communication_lists.len(),
        "deployment commands should create every communication list"
    );
    assert_eq!(
        create_routes.len(),
        command_routes.len(),
        "deployment commands should create every command route"
    );

    for desk in work_desks {
        let slug = string_field(desk, "slug");
        assert!(
            create_work_desks.iter().any(
                |command| command.starts_with(&format!("/parcel desk create <parcel> {slug} "))
            ),
            "deployment commands should create desk {slug}"
        );
    }
    for list in communication_lists {
        let slug = string_field(list, "slug");
        assert!(
            create_lists.iter().any(|command| command
                .starts_with(&format!("/parcel mailing-list create <parcel> {slug} "))),
            "deployment commands should create communication list {slug}"
        );
    }
    for route in command_routes {
        let slug = string_field(route, "targetDesk");
        let prefix = string_field(route, "commandPrefix");
        let expected = format!("/parcel route add <parcel> {slug} {prefix}");
        assert!(
            create_routes.iter().any(|command| command == &expected),
            "deployment commands should include exact route command {expected}"
        );
    }
}

fn assert_job_guides_are_deployable(blueprint: &Value) {
    let deployment = value_field(blueprint, "deploymentCommands");
    let publish_commands = string_array_field(deployment, "publishJobGuides");
    let guides = array_field(value_field(blueprint, "jobGuides"), "jobGuides");
    let guide_slugs = slug_set(guides, "slug");

    for required in ["chief-editor", "editor", "reporter", "contributor"] {
        assert!(
            guide_slugs.contains(required),
            "jobGuides should include {required}"
        );
    }
    assert_eq!(
        publish_commands.len(),
        guides.len(),
        "deployment commands should publish every job guide"
    );
    for guide in guides {
        let slug = string_field(guide, "slug");
        let title = string_field(guide, "title");
        let body = string_field(guide, "body");
        let read_command = string_field(guide, "readCommand");
        let publish_command = string_field(guide, "publishCommand");
        assert!(!title.is_empty(), "job guide {slug} should have a title");
        assert!(!body.is_empty(), "job guide {slug} should have a body");
        assert_eq!(
            read_command,
            format!("/parcel job read <parcel> {slug}"),
            "job guide {slug} should define its parcel read command"
        );
        assert!(
            publish_command.starts_with(&format!("/parcel job publish <parcel> {slug} ")),
            "job guide {slug} should define its parcel publish command"
        );
        assert!(
            publish_commands.contains(&publish_command),
            "deployment commands should include exact job publish command for {slug}"
        );
    }

    let manuals = object_field(blueprint, "operatorManuals");
    for required in ["chiefEditor", "editor", "reporter", "contributor"] {
        let Some(manual) = manuals.get(required).and_then(Value::as_str) else {
            panic!("operatorManuals.{required} should be a string");
        };
        assert!(
            manual.contains("/parcel job read"),
            "operatorManuals.{required} should point to the runtime JD"
        );
    }
}

fn assert_roles_target_existing_desks_and_lists(blueprint: &Value) {
    let desks = slug_set(array_field(&blueprint["workDesks"], "workDesks"), "slug");
    let lists = slug_set(
        array_field(&blueprint["communicationLists"], "communicationLists"),
        "slug",
    );

    for (role, values) in object_field(blueprint, "roleWorkDesks") {
        let role_desks = values
            .as_array()
            .unwrap_or_else(|| panic!("roleWorkDesks.{role} should be an array"));
        for desk in role_desks {
            let slug = desk
                .as_str()
                .unwrap_or_else(|| panic!("roleWorkDesks.{role} contains a non-string desk"));
            assert!(
                desks.contains(slug),
                "roleWorkDesks.{role} targets unknown desk {slug}"
            );
        }
    }

    for (role, values) in object_field(blueprint, "roleCommunicationLists") {
        let role_lists = values
            .as_array()
            .unwrap_or_else(|| panic!("roleCommunicationLists.{role} should be an array"));
        for list in role_lists {
            let slug = list.as_str().unwrap_or_else(|| {
                panic!("roleCommunicationLists.{role} contains a non-string list")
            });
            assert!(
                lists.contains(slug),
                "roleCommunicationLists.{role} targets unknown list {slug}"
            );
        }
    }
}

fn assert_validation_tests_define_storyline_acceptance(blueprint: &Value) {
    let validation = value_field(blueprint, "validationTests");
    let requirements = string_array_field(validation, "runnerRequirements").join("\n");
    for required in [
        "distinct player identities",
        "fresh mail-protocol Agent pool lease",
        "fresh SSH session inside the parcel",
        "outside core storage",
    ] {
        assert!(
            requirements.contains(required),
            "validation runner requirements should mention {required}"
        );
    }

    let validation_text = value_field(blueprint, "validationTests").to_string();
    let storylines = array_field(
        value_field(validation, "storylines"),
        "validationTests.storylines",
    );
    let storyline_ids = storylines
        .iter()
        .map(|storyline| string_field(storyline, "id").to_owned())
        .collect::<BTreeSet<_>>();
    for required in [
        "first-edition-day",
        "second-day-missed-worker-recovery",
        "unused-reporter-article",
        "third-day-performance-dismissal",
    ] {
        assert!(
            storyline_ids.contains(required),
            "validationTests should include storyline {required}"
        );
    }

    for storyline in storylines {
        let id = string_field(storyline, "id");
        assert!(
            !string_field(storyline, "premise").is_empty(),
            "validation storyline {id} should state a premise"
        );
        assert_storyline_actors(id, storyline);
        assert_storyline_chapters(id, storyline);
        assert!(
            !string_array_field(storyline, "finalAcceptance").is_empty(),
            "validation storyline {id} should contain final acceptance criteria"
        );
        assert!(
            !string_array_field(storyline, "failureConditions").is_empty(),
            "validation storyline {id} should contain failure conditions"
        );
    }

    for required in [
        "first daily issue",
        "Harbor Voices",
        "65 MARK",
        "weekly-reader",
        "hermes-poor-editor",
        "hermes-poor-reporter",
        "/parcel staff remove",
        "/parcel job publish",
        "/parcel job read",
        "Chief Editor JD",
        "Editor JD",
        "Reporter JD",
        "Contributor JD",
        "not-used",
        "removed",
        "reapplied",
        "revise",
        "void",
        "settled",
        "stateTests",
        "chiefEditor may remove editors",
        "chiefEditor may remove reporters",
        "editor may decide not to use a reporter article",
        "hermes-editor",
        "hermes-reporter",
        "mail-agent pool lease",
        "SSH session",
        "weekly unsubscribe",
        "debt ledger",
        "idempotent",
        "command_id",
        "work_item_id",
    ] {
        assert!(
            validation_text.contains(required),
            "validation tests should cover {required}"
        );
    }
}

fn assert_validation_tests_cover_workflow_states(blueprint: &Value) {
    let validation = value_field(blueprint, "validationTests");
    let state_machines = array_field(
        value_field(validation, "workflowStateCoverage"),
        "validationTests.workflowStateCoverage",
    );
    let workflows = state_machines
        .iter()
        .map(|workflow| string_field(workflow, "id").to_owned())
        .collect::<BTreeSet<_>>();
    for required in [
        "staff-lifecycle",
        "reporter-article-lifecycle",
        "submission-lifecycle",
        "worker-presence-lifecycle",
        "ledger-lifecycle",
        "weekly-subscription-lifecycle",
    ] {
        assert!(
            workflows.contains(required),
            "workflowStateCoverage should include workflow {required}"
        );
    }

    let storylines = array_field(
        value_field(validation, "storylines"),
        "validationTests.storylines",
    );
    let storyline_ids = storylines
        .iter()
        .map(|storyline| string_field(storyline, "id").to_owned())
        .collect::<BTreeSet<_>>();

    for workflow in state_machines {
        let id = string_field(workflow, "id");
        assert!(
            !string_array_field(workflow, "states").is_empty(),
            "workflow {id} should list states"
        );
        assert!(
            !string_array_field(workflow, "transitions").is_empty(),
            "workflow {id} should list transitions"
        );
        for tested_by in string_array_field(workflow, "testedBy") {
            assert!(
                storyline_ids.contains(tested_by),
                "workflow {id} is tested by unknown storyline {tested_by}"
            );
        }
    }

    assert_workflow_states(
        state_machines,
        "staff-lifecycle",
        &["applied", "active", "probation", "removed", "reapplied"],
    );
    assert_workflow_states(
        state_machines,
        "reporter-article-lifecycle",
        &[
            "assigned",
            "filed",
            "under-editorial-review",
            "used-in-daily",
            "not-used",
            "revision-requested",
            "archived",
        ],
    );
    assert_workflow_states(
        state_machines,
        "submission-lifecycle",
        &[
            "submitted",
            "routed",
            "claimed",
            "approved",
            "rejected",
            "revision-requested",
            "published",
            "closed",
        ],
    );
    assert_workflow_states(
        state_machines,
        "worker-presence-lifecycle",
        &[
            "assigned",
            "outside-parcel",
            "inside-parcel-no-mail-lease",
            "eligible",
            "stale",
            "returned",
        ],
    );
    assert_workflow_states(
        state_machines,
        "ledger-lifecycle",
        &["none", "owed", "void", "settled"],
    );
    assert_workflow_states(
        state_machines,
        "weekly-subscription-lifecycle",
        &[
            "default-subscribed",
            "opt-out-requested",
            "opted-out",
            "excluded-from-delivery",
        ],
    );
}

fn assert_workflow_states(workflows: &[Value], id: &str, required_states: &[&str]) {
    let workflow = workflows
        .iter()
        .find(|workflow| string_field(workflow, "id") == id)
        .unwrap_or_else(|| panic!("missing workflow {id}"));
    let states = string_array_field(workflow, "states")
        .into_iter()
        .collect::<BTreeSet<_>>();
    let state_tests = object_field(workflow, "stateTests");
    for required_state in required_states {
        assert!(
            states.contains(required_state),
            "workflow {id} should include state {required_state}"
        );
        let state_test = state_tests
            .get(*required_state)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("workflow {id} should define stateTests.{required_state}"));
        assert!(
            !state_test.is_empty(),
            "workflow {id} should describe how state {required_state} is tested"
        );
    }
}

fn assert_storyline_actors(id: &str, storyline: &Value) {
    let actors = string_array_field(storyline, "actors");
    assert!(
        actors.len() >= 3,
        "validation storyline {id} should involve multiple actors"
    );
    if id == "first-edition-day" {
        for required in [
            "hermes-chief-editor",
            "hermes-editor",
            "hermes-reporter",
            "newspaper-contributor",
            "weekly-reader",
        ] {
            assert!(
                actors.contains(&required),
                "first-edition-day should include actor {required}"
            );
        }
    }
}

fn assert_storyline_chapters(id: &str, storyline: &Value) {
    let chapters = array_field(value_field(storyline, "chapters"), "storyline.chapters");
    assert!(
        chapters.len() >= 2,
        "validation storyline {id} should contain multiple chapters"
    );
    for chapter in chapters {
        let chapter_id = string_field(chapter, "id");
        assert!(
            !string_field(chapter, "narrative").is_empty(),
            "chapter {chapter_id} should explain its narrative role"
        );
        assert!(
            !string_array_field(chapter, "commands").is_empty(),
            "chapter {chapter_id} should contain concrete commands"
        );
        assert!(
            !string_array_field(chapter, "expected").is_empty(),
            "chapter {chapter_id} should contain expected story outcomes"
        );
    }
}

fn slug_set(values: &[Value], field: &str) -> BTreeSet<String> {
    values
        .iter()
        .map(|value| string_field(value, field).to_owned())
        .collect()
}

fn value_field<'a>(value: &'a Value, field: &str) -> &'a Value {
    value
        .get(field)
        .unwrap_or_else(|| panic!("missing JSON field {field}"))
}

fn object_field<'a>(value: &'a Value, field: &str) -> &'a serde_json::Map<String, Value> {
    value_field(value, field)
        .as_object()
        .unwrap_or_else(|| panic!("JSON field {field} should be an object"))
}

fn array_field<'a>(value: &'a Value, field: &str) -> &'a Vec<Value> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("JSON field {field} should be an array"))
}

fn string_array_field<'a>(value: &'a Value, field: &str) -> Vec<&'a str> {
    value_field(value, field)
        .as_array()
        .unwrap_or_else(|| panic!("JSON field {field} should be an array"))
        .iter()
        .map(|item| {
            item.as_str()
                .unwrap_or_else(|| panic!("JSON field {field} should contain only strings"))
        })
        .collect()
}

fn string_field<'a>(value: &'a Value, field: &str) -> &'a str {
    value_field(value, field)
        .as_str()
        .unwrap_or_else(|| panic!("JSON field {field} should be a string"))
}

fn number_field(value: &Value, field: &str) -> i64 {
    value_field(value, field)
        .as_i64()
        .unwrap_or_else(|| panic!("JSON field {field} should be a number"))
}

fn bool_field(value: &Value, field: &str) -> bool {
    value_field(value, field)
        .as_bool()
        .unwrap_or_else(|| panic!("JSON field {field} should be a boolean"))
}

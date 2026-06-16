use super::*;

#[test]
fn admission_guidance_points_to_agreement_board() {
    let app = AppService::new(TestAdmissionStore {
        admission: Mutex::new(TestAdmission {
            admission_state: ADMISSION_STATE_PENDING.to_owned(),
            agreement_version: None,
            agreement_read_version: None,
        }),
    });
    let guidance = app.admission_guidance(&PendingAdmission);

    assert!(guidance.contains("Admission pending"));
    assert!(guidance.contains("/read agreement"));
}

#[test]
fn pending_admission_observation_is_restricted_to_safe_commands() {
    let mut observation = JsonObservation {
        player_id: "player".to_owned(),
        view_id: "arrival_street".to_owned(),
        title: "Arrival".to_owned(),
        ascii_art: Vec::new(),
        description: "Original description.".to_owned(),
        exits: vec![hinemos_core::ExitObservation {
            direction: hinemos_core::Direction::North,
            target_known: true,
            label: None,
        }],
        entities: Vec::new(),
        online_users: Vec::new(),
        available_commands: vec![SemanticCommand::Inventory],
        events: Vec::new(),
    };

    let app = AppService::new(TestAdmissionStore {
        admission: Mutex::new(TestAdmission {
            admission_state: ADMISSION_STATE_PENDING.to_owned(),
            agreement_version: None,
            agreement_read_version: None,
        }),
    });

    app.restrict_pending_admission_observation(
        &mut observation,
        &PendingAdmission,
        "agreement_board",
    );

    assert!(observation.description.contains("Original description."));
    assert!(observation.description.contains("Admission pending"));
    assert!(observation.exits.is_empty());
    assert_eq!(
        observation.available_commands,
        vec![
            SemanticCommand::Look,
            SemanticCommand::Read {
                target: EntityRef::new("agreement_board"),
            },
            SemanticCommand::Help,
            SemanticCommand::Quit,
        ]
    );
}

#[test]
fn pending_admission_read_returns_next_step_from_app() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let store = TestAdmissionStore {
            admission: Mutex::new(TestAdmission {
                admission_state: ADMISSION_STATE_PENDING.to_owned(),
                agreement_version: None,
                agreement_read_version: None,
            }),
        };
        let app = AppService::new(store);

        let events = app
            .handle_pending_admission_read("player")
            .await
            .expect("pending admission read");

        assert_eq!(
            events,
            vec![UiEvent::Text(
                "\r\nNext step: type /agree to enter.\r\n".to_owned()
            )]
        );
        assert_eq!(
            app.store()
                .admission
                .lock()
                .unwrap()
                .agreement_read_version
                .as_deref(),
            Some(hinemos_core::DEFAULT_AGREEMENT_VERSION)
        );
    });
}

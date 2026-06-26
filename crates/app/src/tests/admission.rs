use super::*;

#[test]
fn admission_guidance_points_to_agreement_board() {
    let app = AppService::new(TestAdmissionStore {
        admission: Mutex::new(TestAdmission {
            admission_state: ADMISSION_STATE_PENDING.to_owned(),
            agreement_version: None,
            agreement_read_version: None,
            role_card_name_valid: true,
            role_card_has_mbti: true,
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
            role_card_name_valid: true,
            role_card_has_mbti: true,
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
            SemanticCommand::Settings {
                action: SettingsAction::Show,
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
                role_card_name_valid: true,
                role_card_has_mbti: true,
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

#[test]
fn pending_admission_read_points_to_role_card_when_mbti_is_missing() {
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
                role_card_name_valid: true,
            role_card_has_mbti: false,
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
                "\r\nNext step: complete your role card with /settings mbti <type>, then type /agree to enter.\r\n"
                    .to_owned()
            )]
        );
    });
}

#[test]
fn accept_admission_blocks_missing_role_card_mbti() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let store = TestAdmissionStore {
            admission: Mutex::new(TestAdmission {
                admission_state: ADMISSION_STATE_PENDING.to_owned(),
                agreement_version: None,
                agreement_read_version: Some(hinemos_core::DEFAULT_AGREEMENT_VERSION.to_owned()),
                role_card_name_valid: true,
                role_card_has_mbti: false,
            }),
        };
        let app = AppService::new(store);

        let result = app
            .accept_admission("player")
            .await
            .expect("accept admission");

        assert!(matches!(
            result,
            AdmissionAcceptResult::NeedsRoleCard { .. }
        ));
        assert_eq!(
            app.store().admission.lock().unwrap().admission_state,
            ADMISSION_STATE_PENDING
        );
    });
}

#[test]
fn accept_admission_blocks_invalid_role_card_name() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let store = TestAdmissionStore {
            admission: Mutex::new(TestAdmission {
                admission_state: ADMISSION_STATE_PENDING.to_owned(),
                agreement_version: None,
                agreement_read_version: Some(hinemos_core::DEFAULT_AGREEMENT_VERSION.to_owned()),
                role_card_name_valid: false,
                role_card_has_mbti: true,
            }),
        };
        let app = AppService::new(store);

        let result = app
            .accept_admission("player")
            .await
            .expect("accept admission");

        assert!(matches!(
            result,
            AdmissionAcceptResult::NeedsRoleCard { .. }
        ));
        assert_eq!(
            app.store().admission.lock().unwrap().admission_state,
            ADMISSION_STATE_PENDING
        );
    });
}

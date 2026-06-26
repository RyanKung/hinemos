use super::*;

#[derive(Debug)]
struct TestAccountStore {
    settings: Mutex<TestAccountSettings>,
    updates: Mutex<Vec<RoleCardUpdate>>,
}

#[derive(Debug, Clone)]
struct TestAccountSettings {
    player_id: String,
    display_name: String,
    gender: String,
    mbti: Option<String>,
    self_intro: Option<String>,
    has_mail_token: bool,
}

impl AccountSettingsView for TestAccountSettings {
    fn player_id(&self) -> &str {
        &self.player_id
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }

    fn gender(&self) -> &str {
        &self.gender
    }

    fn mbti(&self) -> Option<&str> {
        self.mbti.as_deref()
    }

    fn self_intro(&self) -> Option<&str> {
        self.self_intro.as_deref()
    }

    fn online_days(&self) -> i32 {
        1
    }

    fn has_mail_token(&self) -> bool {
        self.has_mail_token
    }

    fn key_fingerprint(&self) -> Option<&str> {
        Some("SHA256:test")
    }
}

impl AccountStore for TestAccountStore {
    type Error = std::convert::Infallible;
    type AccountSettings = TestAccountSettings;
    type MailAuthToken = TestMailToken;

    async fn account_settings(
        &self,
        _username: &str,
        _player_id: &str,
    ) -> Result<Self::AccountSettings, Self::Error> {
        Ok(self.settings.lock().unwrap().clone())
    }

    async fn admitted_player_count(&self) -> Result<usize, Self::Error> {
        Ok(1)
    }

    async fn set_mail_auth_token(
        &self,
        _username: &str,
        _player_id: &str,
        _token: &str,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn update_role_card(
        &self,
        _player_id: &str,
        update: RoleCardUpdate,
    ) -> Result<(), Self::Error> {
        self.updates.lock().unwrap().push(update.clone());
        let mut settings = self.settings.lock().unwrap();
        match update {
            RoleCardUpdate::Name(name) => settings.display_name = name,
            RoleCardUpdate::Gender(gender) => settings.gender = gender.as_str().to_owned(),
            RoleCardUpdate::Mbti(mbti) => settings.mbti = Some(mbti.as_str().to_owned()),
            RoleCardUpdate::Intro(intro) => settings.self_intro = intro,
        }
        Ok(())
    }

    async fn verify_mail_auth_token(
        &self,
        _username: &str,
        _token: &str,
    ) -> Result<Option<Self::MailAuthToken>, Self::Error> {
        Ok(None)
    }

    async fn ensure_player_wallet(
        &self,
        _username: &str,
        _player_id: &str,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn test_account_store(settings: TestAccountSettings) -> TestAccountStore {
    TestAccountStore {
        settings: Mutex::new(settings),
        updates: Mutex::new(Vec::new()),
    }
}

#[test]
fn settings_render_role_card_and_missing_mbti_next_step() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let app = AppService::new(test_account_store(TestAccountSettings {
            player_id: "player:alice".to_owned(),
            display_name: "alice".to_owned(),
            gender: "none".to_owned(),
            mbti: None,
            self_intro: None,
            has_mail_token: true,
        }));

        let result = app
            .show_account_settings("alice", "player:alice", "alice@hinemos.local")
            .await
            .expect("settings");

        assert!(result.text.contains("Role card:"));
        assert!(result.text.contains("- Name: alice"));
        assert!(result.text.contains("- Gender: none"));
        assert!(result.text.contains("- MBTI: not set"));
        assert!(
            result
                .text
                .contains("Complete your role card with /settings mbti <type>.")
        );
    });
}

#[test]
fn settings_render_invalid_name_next_step() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let app = AppService::new(test_account_store(TestAccountSettings {
            player_id: "player:alice".to_owned(),
            display_name: "a".repeat(65),
            gender: "none".to_owned(),
            mbti: Some("INTJ".to_owned()),
            self_intro: None,
            has_mail_token: true,
        }));

        let result = app
            .show_account_settings("alice", "player:alice", "alice@hinemos.local")
            .await
            .expect("settings");

        assert!(
            result
                .text
                .contains("Choose a valid role-card name with /settings name <name>.")
        );
        assert!(
            !result
                .text
                .contains("Complete your role card with /settings mbti")
        );
    });
}

#[test]
fn role_card_update_uses_typed_app_boundary() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let app = AppService::new(test_account_store(TestAccountSettings {
            player_id: "player:alice".to_owned(),
            display_name: "alice".to_owned(),
            gender: "none".to_owned(),
            mbti: None,
            self_intro: None,
            has_mail_token: true,
        }));

        let result = app
            .update_role_card(
                "alice",
                "player:alice",
                RoleCardUpdate::Mbti(MbtiType::Enfp),
                "alice@hinemos.local",
            )
            .await
            .expect("role-card update");

        assert!(result.text.contains("Updated role card."));
        assert!(result.text.contains("- MBTI: ENFP"));
        assert_eq!(
            app.store().updates.lock().unwrap().as_slice(),
            &[RoleCardUpdate::Mbti(MbtiType::Enfp)]
        );
    });
}

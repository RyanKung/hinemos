use crate::*;

impl<S, E> AppService<S>
where
    S: AdmissionStore<Error = E>,
{
    /// Loads the current admission state for a player.
    pub async fn player_admission(&self, player_id: &str) -> Result<S::Admission, E> {
        self.store.player_admission(player_id).await
    }

    /// Records that the player read the active admission board.
    pub async fn read_admission_agreement(&self, player_id: &str) -> Result<(), E> {
        self.store
            .mark_agreement_read(player_id, &self.config.agreement_version)
            .await
    }

    /// Returns the next-step text after reading the admission agreement, if needed.
    #[must_use]
    pub fn admission_next_step_after_read(&self, admission: &impl AdmissionView) -> Option<String> {
        if admission.is_agreed() || !admission.has_read_version(&self.config.agreement_version) {
            return None;
        }
        if admission.role_card_is_complete() {
            Some("\r\nNext step: type /agree to enter.\r\n".to_owned())
        } else {
            Some(format!(
                "\r\nNext step: {}, then type /agree to enter.\r\n",
                admission_role_card_next_step(admission),
            ))
        }
    }

    /// Builds the successful admission text after wallet setup.
    #[must_use]
    pub fn admission_accepted_text(
        &self,
        agreement_version: &str,
        amount: i64,
        asset: &str,
    ) -> String {
        format!(
            "Agreement accepted: version {agreement_version}. Initial grant issued: {amount} {asset}. Welcome to Hinemos.\r\n"
        )
    }

    /// Returns the player-facing guidance for a pending admission.
    #[must_use]
    pub fn admission_guidance(&self, admission: &impl AdmissionView) -> String {
        let mut steps = Vec::new();
        if !admission.has_read_version(&self.config.agreement_version) {
            steps.push("Read the board agreement first: /read agreement");
        }
        if !admission.role_card_name_is_valid() {
            steps.push("Choose a valid role-card name: /settings name <name>");
        }
        if !admission.role_card_has_mbti() {
            steps.push("Complete your role card: /settings mbti <type>");
        }
        let next_step = if steps.is_empty() {
            "Type /agree to enter".to_owned()
        } else {
            steps.join(". ")
        };
        format!(
            "Admission pending. SSH authentication is complete, but this account is not admitted into the world yet.\n{next_step}. Until then, other commands are blocked."
        )
    }

    /// Restricts an observation to admission-safe commands while a player is pending.
    pub fn restrict_pending_admission_observation(
        &self,
        observation: &mut JsonObservation,
        admission: &impl AdmissionView,
        admission_board_entity_id: &str,
    ) {
        observation.description = format!(
            "{}\n\n{}",
            observation.description,
            self.admission_guidance(admission)
        );
        observation.exits.clear();
        observation.available_commands = vec![
            SemanticCommand::Look,
            SemanticCommand::Read {
                target: EntityRef::new(admission_board_entity_id),
            },
            SemanticCommand::Settings {
                action: SettingsAction::Show,
            },
            SemanticCommand::Help,
            SemanticCommand::Quit,
        ];
    }

    /// Records that the player read the active admission board and returns the next-step event, if any.
    pub async fn handle_pending_admission_read(&self, player_id: &str) -> Result<Vec<UiEvent>, E> {
        self.read_admission_agreement(player_id).await?;
        let admission = self.player_admission(player_id).await?;
        Ok(self
            .admission_next_step_after_read(&admission)
            .map(|text| vec![UiEvent::Text(text)])
            .unwrap_or_default())
    }
}

/// Storage boundary for admission agreement state.
pub trait AdmissionStore {
    /// Store error type.
    type Error;
    /// Stored admission type.
    type Admission: AdmissionView;

    /// Loads admission state for a player.
    async fn player_admission(&self, player_id: &str) -> Result<Self::Admission, Self::Error>;

    /// Records that a player has read an agreement version.
    async fn mark_agreement_read(
        &self,
        player_id: &str,
        agreement_version: &str,
    ) -> Result<(), Self::Error>;

    /// Marks a player as admitted under an agreement version.
    async fn admit_player(
        &self,
        player_id: &str,
        agreement_version: &str,
    ) -> Result<(), Self::Error>;
}

/// Protocol-neutral view of a player's admission state.
pub trait AdmissionView {
    /// Returns true when the player has accepted the active admission agreement.
    fn is_agreed(&self) -> bool;

    /// Returns true when the player has read the given agreement version.
    fn has_read_version(&self, version: &str) -> bool;

    /// Returns true when the role-card name satisfies profile rules.
    fn role_card_name_is_valid(&self) -> bool;

    /// Returns true when the role-card has an MBTI type.
    fn role_card_has_mbti(&self) -> bool;

    /// Returns true when required role-card fields are complete.
    fn role_card_is_complete(&self) -> bool {
        self.role_card_name_is_valid() && self.role_card_has_mbti()
    }
}

/// Result from attempting to accept admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionAcceptResult {
    /// Player had already accepted admission.
    AlreadyAgreed {
        /// Text to display to the player.
        text: String,
    },
    /// Player must read the active agreement first.
    NeedsRead {
        /// Text to display to the player.
        text: String,
    },
    /// Player must complete required role-card fields first.
    NeedsRoleCard {
        /// Text to display to the player.
        text: String,
    },
    /// Admission was accepted and caller should finish post-admission setup.
    Accepted,
}

fn admission_role_card_next_step(admission: &impl AdmissionView) -> &'static str {
    match (
        admission.role_card_name_is_valid(),
        admission.role_card_has_mbti(),
    ) {
        (false, false) => {
            "complete your role card with /settings name <name> and /settings mbti <type>"
        }
        (false, true) => "choose a valid role-card name with /settings name <name>",
        (true, false) => "complete your role card with /settings mbti <type>",
        (true, true) => "type /agree",
    }
}

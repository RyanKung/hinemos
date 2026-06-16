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
        (!admission.is_agreed() && admission.has_read_version(&self.config.agreement_version))
            .then(|| "\r\nNext step: type /agree to enter.\r\n".to_owned())
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
    pub fn admission_guidance(&self, _admission: &impl AdmissionView) -> String {
        let next_step = "Read the board agreement first: /read agreement";
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
    /// Admission was accepted and caller should finish post-admission setup.
    Accepted,
}

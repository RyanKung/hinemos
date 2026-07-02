use super::*;
use crate::{NewMemoryAtom, NewMemoryEvent};
use dadoes::{DadoesClassifier, EmotionClassifier, Mood, MoodScore, train_seed_model};
use serde_json::json;
use std::sync::OnceLock;

impl MemoryAtomView for StoredMemoryAtom {
    fn subject(&self) -> &str {
        &self.subject
    }

    fn predicate(&self) -> &str {
        &self.predicate
    }

    fn object(&self) -> &Value {
        &self.object
    }

    fn summary(&self) -> &str {
        &self.summary
    }
}

impl MemoryEventView for StoredMemoryEvent {
    fn source(&self) -> &str {
        &self.source
    }

    fn event_type(&self) -> &str {
        &self.event_type
    }

    fn content(&self) -> &str {
        &self.content
    }
}

impl SocialEdgeView for StoredSocialEdge {
    fn trust(&self) -> f64 {
        self.trust
    }

    fn affinity(&self) -> f64 {
        self.affinity
    }

    fn obligation(&self) -> f64 {
        self.obligation
    }

    fn rivalry(&self) -> f64 {
        self.rivalry
    }

    fn familiarity(&self) -> f64 {
        self.familiarity
    }

    fn tags(&self) -> &[String] {
        &self.tags
    }
}

impl SelfModelView for StoredAgentSelfModel {
    fn version(&self) -> i64 {
        self.version
    }

    fn created_at(&self) -> &str {
        &self.created_at
    }

    fn identity(&self) -> &Value {
        &self.identity
    }

    fn current_state(&self) -> &Value {
        &self.current_state
    }

    fn style(&self) -> &Value {
        &self.style
    }
}

impl MemoryStore for PgStorage {
    type Error = StorageError;
    type MemoryAtom = StoredMemoryAtom;
    type MemoryEvent = StoredMemoryEvent;
    type SocialEdge = StoredSocialEdge;
    type SelfModel = StoredAgentSelfModel;

    async fn latest_self_model(
        &self,
        agent_id: &str,
    ) -> Result<Option<Self::SelfModel>, Self::Error> {
        PgStorage::latest_self_model(self, agent_id).await
    }

    async fn ensure_self_model(
        &self,
        agent_id: &str,
        identity: &Value,
        current_state: &Value,
        style: &Value,
    ) -> Result<Self::SelfModel, Self::Error> {
        PgStorage::ensure_self_model(self, agent_id, identity, current_state, style).await
    }

    async fn record_self_model_state(
        &self,
        agent_id: &str,
        current_state: &Value,
    ) -> Result<Self::SelfModel, Self::Error> {
        PgStorage::record_self_model_state(self, agent_id, current_state).await
    }

    async fn record_daily_report(&self, agent_id: &str, content: &str) -> Result<(), Self::Error> {
        let emotion = daily_report_emotion(content);
        let event = PgStorage::append_memory_event(
            self,
            NewMemoryEvent {
                agent_id: agent_id.to_owned(),
                source: "daily_report".to_owned(),
                event_type: "resident_daily_report".to_owned(),
                actors: json!([agent_id]),
                content: content.to_owned(),
                world_refs: json!({}),
                salience: 0.7,
            },
        )
        .await?;
        PgStorage::upsert_memory_atom(
            self,
            NewMemoryAtom {
                agent_id: agent_id.to_owned(),
                kind: "self".to_owned(),
                subject: agent_id.to_owned(),
                predicate: "last_daily_report".to_owned(),
                object: json!({
                    "eventId": event.id,
                    "content": content,
                    "emotion": emotion.object,
                }),
                summary: format!("Daily report: {content}"),
                evidence_event_ids: vec![event.id],
                confidence: 0.8,
                importance: 0.7,
                emotional_valence: emotion.valence,
            },
        )
        .await?;
        Ok(())
    }

    async fn search_memory_atoms(
        &self,
        agent_id: &str,
        query: Option<&str>,
        kind: Option<&str>,
        subject: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Self::MemoryAtom>, Self::Error> {
        PgStorage::search_memory_atoms(self, agent_id, query, kind, subject, limit).await
    }

    async fn search_memory_events(
        &self,
        agent_id: &str,
        query: Option<&str>,
        event_type: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Self::MemoryEvent>, Self::Error> {
        PgStorage::search_memory_events(self, agent_id, query, event_type, limit).await
    }

    async fn recall_person_memory(
        &self,
        agent_id: &str,
        person_id: &str,
        limit: i64,
    ) -> Result<Vec<Self::MemoryAtom>, Self::Error> {
        PgStorage::recall_person_memory(self, agent_id, person_id, limit).await
    }

    async fn social_edge(
        &self,
        agent_id: &str,
        target_id: &str,
    ) -> Result<Option<Self::SocialEdge>, Self::Error> {
        PgStorage::social_edge(self, agent_id, target_id).await
    }
}

struct DailyReportEmotion {
    object: Value,
    valence: f64,
}

fn daily_report_emotion(content: &str) -> DailyReportEmotion {
    let Some((classifier, model_source)) = dadoes_classifier() else {
        return DailyReportEmotion {
            object: json!({
                "status": "unavailable",
                "model": "dadoes",
            }),
            valence: 0.0,
        };
    };
    let analysis = classifier.classify(content);
    let primary = analysis.primary_mood();
    let active_moods = classifier
        .active_moods(&analysis)
        .map(mood_score_json)
        .collect::<Vec<_>>();
    DailyReportEmotion {
        object: json!({
            "status": "scored",
            "model": "dadoes",
            "modelSource": model_source,
            "threshold": classifier.active_mood_threshold(),
            "primaryMood": primary.map(mood_score_json),
            "activeMoods": active_moods,
        }),
        valence: primary.map_or(0.0, mood_valence),
    }
}

fn dadoes_classifier() -> Option<(&'static DadoesClassifier, &'static str)> {
    static CLASSIFIER: OnceLock<Option<(DadoesClassifier, &'static str)>> = OnceLock::new();
    CLASSIFIER
        .get_or_init(load_dadoes_classifier)
        .as_ref()
        .map(|(classifier, source)| (classifier, *source))
}

fn load_dadoes_classifier() -> Option<(DadoesClassifier, &'static str)> {
    DadoesClassifier::from_default_model()
        .map(|classifier| (classifier, "default_checkpoint"))
        .or_else(|_| {
            train_seed_model()
                .map(DadoesClassifier::from_model)
                .map(|classifier| (classifier, "seed_fallback"))
        })
        .ok()
}

fn mood_score_json(score: MoodScore) -> Value {
    json!({
        "mood": score.mood.as_str(),
        "score": score.score,
    })
}

fn mood_valence(score: MoodScore) -> f64 {
    let polarity = match score.mood {
        Mood::Happy | Mood::Satisfied | Mood::Excited | Mood::Curious | Mood::Hopeful => 1.0,
        Mood::Anxious
        | Mood::Frustrated
        | Mood::Sad
        | Mood::Angry
        | Mood::Lonely
        | Mood::Bored
        | Mood::Tired => -1.0,
        Mood::Neutral => 0.0,
    };
    polarity * f64::from(score.score)
}

impl MessageStore for PgStorage {
    type Error = StorageError;
    type WorldMessage = StoredWorldMessage;
    type Balance = StoredBalance;

    async fn recent_view_messages(
        &self,
        view_id: &str,
        limit: i64,
    ) -> Result<Vec<Self::WorldMessage>, Self::Error> {
        PgStorage::recent_view_messages(self, view_id, limit).await
    }

    async fn recent_news_messages(
        &self,
        limit: i64,
    ) -> Result<Vec<Self::WorldMessage>, Self::Error> {
        PgStorage::recent_news_messages(self, limit).await
    }

    async fn player_balance(&self, player_id: &str) -> Result<Self::Balance, Self::Error> {
        PgStorage::player_balance(self, player_id).await
    }

    async fn save_say_message(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        target_view: &str,
        body: &str,
    ) -> Result<(), Self::Error> {
        PgStorage::save_say_message(self, sender_user, sender_player_id, target_view, body).await
    }

    async fn save_mail_message(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        target: &str,
        body: &str,
    ) -> Result<(), Self::Error> {
        PgStorage::save_mail_message(self, sender_user, sender_player_id, target, body).await?;
        Ok(())
    }

    async fn save_mail_message_with_subject(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        target: &str,
        subject: &str,
        body: &str,
    ) -> Result<(), Self::Error> {
        PgStorage::save_mail_message_with_subject(
            self,
            sender_user,
            sender_player_id,
            target,
            subject,
            body,
        )
        .await?;
        Ok(())
    }

    async fn save_broadcast_message(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        body: &str,
    ) -> Result<(), Self::Error> {
        PgStorage::save_broadcast_message(self, sender_user, sender_player_id, body).await?;
        Ok(())
    }
}

impl WorldMessageView for StoredWorldMessage {
    fn kind(&self) -> &str {
        &self.kind
    }

    fn sender_user(&self) -> &str {
        &self.sender_user
    }

    fn body(&self) -> &str {
        &self.body
    }

    fn created_at(&self) -> &str {
        &self.created_at
    }

    fn expires_at(&self) -> Option<&str> {
        self.expires_at.as_deref()
    }
}

impl BalanceView for StoredBalance {
    fn account_id(&self) -> &str {
        &self.account_id
    }

    fn asset(&self) -> &str {
        &self.asset
    }

    fn amount(&self) -> i64 {
        self.amount
    }
}

impl MailStore for PgStorage {
    type Error = StorageError;
    type InboxItem = StoredInboxItem;

    async fn save_room_mailbox_input<M>(
        &self,
        mailbox: &M,
        sender_user: &str,
        sender_player_id: &str,
        raw_input: &str,
    ) -> Result<Self::InboxItem, Self::Error>
    where
        M: RoomMailboxView + Sync,
    {
        PgStorage::save_room_mailbox_input(self, mailbox, sender_user, sender_player_id, raw_input)
            .await
    }
}

impl AdmissionStore for PgStorage {
    type Error = StorageError;
    type Admission = StoredAdmission;

    async fn player_admission(&self, player_id: &str) -> Result<Self::Admission, Self::Error> {
        PgStorage::player_admission(self, player_id).await
    }

    async fn mark_agreement_read(
        &self,
        player_id: &str,
        agreement_version: &str,
    ) -> Result<(), Self::Error> {
        PgStorage::mark_agreement_read(self, player_id, agreement_version).await
    }

    async fn admit_player(
        &self,
        player_id: &str,
        agreement_version: &str,
    ) -> Result<(), Self::Error> {
        PgStorage::admit_player(self, player_id, agreement_version).await
    }
}

impl AdmissionView for StoredAdmission {
    fn is_agreed(&self) -> bool {
        StoredAdmission::is_agreed(self)
    }

    fn has_read_version(&self, version: &str) -> bool {
        StoredAdmission::has_read_version(self, version)
    }

    fn role_card_name_is_valid(&self) -> bool {
        StoredAdmission::role_card_name_is_valid(self)
    }

    fn role_card_has_mbti(&self) -> bool {
        StoredAdmission::role_card_has_mbti(self)
    }
}

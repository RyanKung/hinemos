use crate::{
    NewMemoryAtom, NewMemoryEvent, PgStorage, StorageError, StoredAgentSelfModel, StoredMemoryAtom,
    StoredMemoryEvent, StoredSocialEdge,
};
use serde_json::Value;

impl PgStorage {
    /// Appends an immutable memory event.
    pub async fn append_memory_event(
        &self,
        event: NewMemoryEvent,
    ) -> Result<StoredMemoryEvent, StorageError> {
        let stored = sqlx::query_as::<_, StoredMemoryEvent>(
            r#"
            insert into memory_events (
                agent_id, source, event_type, actors, content, world_refs, salience
            )
            values ($1, $2, $3, $4, $5, $6, $7)
            returning id, agent_id,
                      to_char(occurred_at, 'YYYY-MM-DD HH24:MI:SS TZ') as occurred_at,
                      source, event_type, actors, content, world_refs, salience,
                      to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            "#,
        )
        .bind(event.agent_id)
        .bind(event.source)
        .bind(event.event_type)
        .bind(event.actors)
        .bind(event.content)
        .bind(event.world_refs)
        .bind(event.salience)
        .fetch_one(&self.pool)
        .await?;

        Ok(stored)
    }

    /// Inserts or merges a semantic memory atom.
    pub async fn upsert_memory_atom(
        &self,
        atom: NewMemoryAtom,
    ) -> Result<StoredMemoryAtom, StorageError> {
        let stored = sqlx::query_as::<_, StoredMemoryAtom>(
            r#"
            insert into memory_atoms (
                agent_id, kind, subject, predicate, object, summary, evidence_event_ids,
                confidence, importance, emotional_valence
            )
            values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            on conflict (agent_id, kind, subject, predicate) do update
            set object = excluded.object,
                summary = excluded.summary,
                evidence_event_ids = (
                    select array(
                        select distinct unnest(memory_atoms.evidence_event_ids || excluded.evidence_event_ids)
                        order by 1
                    )
                ),
                confidence = greatest(memory_atoms.confidence, excluded.confidence),
                importance = greatest(memory_atoms.importance, excluded.importance),
                emotional_valence = excluded.emotional_valence,
                updated_at = now()
            returning id, agent_id, kind, subject, predicate, object, summary, evidence_event_ids,
                      confidence, importance, emotional_valence,
                      to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at,
                      to_char(updated_at, 'YYYY-MM-DD HH24:MI:SS TZ') as updated_at,
                      to_char(expires_at, 'YYYY-MM-DD HH24:MI:SS TZ') as expires_at
            "#,
        )
        .bind(atom.agent_id)
        .bind(atom.kind)
        .bind(atom.subject)
        .bind(atom.predicate)
        .bind(atom.object)
        .bind(atom.summary)
        .bind(atom.evidence_event_ids)
        .bind(atom.confidence)
        .bind(atom.importance)
        .bind(atom.emotional_valence)
        .fetch_one(&self.pool)
        .await?;

        Ok(stored)
    }

    /// Searches semantic memory atoms with structured filters and simple text matching.
    pub async fn search_memory_atoms(
        &self,
        agent_id: &str,
        query: Option<&str>,
        kind: Option<&str>,
        subject: Option<&str>,
        limit: i64,
    ) -> Result<Vec<StoredMemoryAtom>, StorageError> {
        let query_pattern = query.map(|value| format!("%{value}%"));
        let atoms = sqlx::query_as::<_, StoredMemoryAtom>(
            r#"
            select id, agent_id, kind, subject, predicate, object, summary, evidence_event_ids,
                   confidence, importance, emotional_valence,
                   to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at,
                   to_char(updated_at, 'YYYY-MM-DD HH24:MI:SS TZ') as updated_at,
                   to_char(expires_at, 'YYYY-MM-DD HH24:MI:SS TZ') as expires_at
            from memory_atoms
            where agent_id = $1
              and ($2::text is null or summary ilike $2 or subject ilike $2 or predicate ilike $2)
              and ($3::text is null or kind = $3)
              and ($4::text is null or subject = $4)
              and (expires_at is null or expires_at > now())
            order by importance desc, updated_at desc, id desc
            limit $5
            "#,
        )
        .bind(agent_id)
        .bind(query_pattern)
        .bind(kind)
        .bind(subject)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(atoms)
    }

    /// Searches append-only memory events with simple text matching.
    pub async fn search_memory_events(
        &self,
        agent_id: &str,
        query: Option<&str>,
        event_type: Option<&str>,
        limit: i64,
    ) -> Result<Vec<StoredMemoryEvent>, StorageError> {
        let query_pattern = query.map(|value| format!("%{value}%"));
        let events = sqlx::query_as::<_, StoredMemoryEvent>(
            r#"
            select id, agent_id,
                   to_char(occurred_at, 'YYYY-MM-DD HH24:MI:SS TZ') as occurred_at,
                   source, event_type, actors, content, world_refs, salience,
                   to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from memory_events
            where agent_id = $1
              and ($2::text is null or content ilike $2 or source ilike $2 or event_type ilike $2 or actors::text ilike $2)
              and ($3::text is null or event_type = $3)
            order by salience desc, occurred_at desc, id desc
            limit $4
            "#,
        )
        .bind(agent_id)
        .bind(query_pattern)
        .bind(event_type)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(events)
    }

    /// Loads recent public memory events suitable for newspaper summaries.
    pub async fn recent_public_press_events(
        &self,
        limit: i64,
    ) -> Result<Vec<StoredMemoryEvent>, StorageError> {
        let events = sqlx::query_as::<_, StoredMemoryEvent>(
            r#"
            select id, agent_id,
                   to_char(occurred_at, 'YYYY-MM-DD HH24:MI:SS TZ') as occurred_at,
                   source, event_type, actors, content, world_refs, salience,
                   to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from memory_events
            where occurred_at >= now() - interval '24 hours'
              and source in ('broadcast', 'chat')
            order by salience desc, occurred_at desc, id desc
            limit $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(events)
    }

    /// Recalls memory atoms about a person or social identity.
    pub async fn recall_person_memory(
        &self,
        agent_id: &str,
        person_id: &str,
        limit: i64,
    ) -> Result<Vec<StoredMemoryAtom>, StorageError> {
        self.search_memory_atoms(agent_id, None, None, Some(person_id), limit)
            .await
    }

    /// Loads the social graph edge for a person if one exists.
    pub async fn social_edge(
        &self,
        agent_id: &str,
        target_id: &str,
    ) -> Result<Option<StoredSocialEdge>, StorageError> {
        let edge = sqlx::query_as::<_, StoredSocialEdge>(
            r#"
            select agent_id, target_id, trust, affinity, obligation, rivalry, familiarity,
                   tags, evidence_memory_ids,
                   to_char(updated_at, 'YYYY-MM-DD HH24:MI:SS TZ') as updated_at
            from social_edges
            where agent_id = $1 and target_id = $2
            "#,
        )
        .bind(agent_id)
        .bind(target_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(edge)
    }

    /// Merges a lightweight social relationship update from a memory atom.
    pub async fn touch_social_edge(
        &self,
        agent_id: &str,
        target_id: &str,
        memory_id: i64,
        tag: Option<&str>,
    ) -> Result<StoredSocialEdge, StorageError> {
        let tags = tag.map_or_else(Vec::new, |value| vec![value.to_string()]);
        let edge = sqlx::query_as::<_, StoredSocialEdge>(
            r#"
            insert into social_edges (
                agent_id, target_id, familiarity, tags, evidence_memory_ids
            )
            values ($1, $2, 0.1, $3, array[$4]::bigint[])
            on conflict (agent_id, target_id) do update
            set familiarity = least(1.0, social_edges.familiarity + 0.05),
                tags = (
                    select array(
                        select distinct unnest(social_edges.tags || excluded.tags)
                        order by 1
                    )
                ),
                evidence_memory_ids = (
                    select array(
                        select distinct unnest(social_edges.evidence_memory_ids || excluded.evidence_memory_ids)
                        order by 1
                    )
                ),
                updated_at = now()
            returning agent_id, target_id, trust, affinity, obligation, rivalry, familiarity,
                      tags, evidence_memory_ids,
                      to_char(updated_at, 'YYYY-MM-DD HH24:MI:SS TZ') as updated_at
            "#,
        )
        .bind(agent_id)
        .bind(target_id)
        .bind(tags)
        .bind(memory_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(edge)
    }

    /// Loads the latest self-model snapshot for an agent.
    pub async fn latest_self_model(
        &self,
        agent_id: &str,
    ) -> Result<Option<StoredAgentSelfModel>, StorageError> {
        let model = sqlx::query_as::<_, StoredAgentSelfModel>(
            r#"
            select agent_id, version, identity, current_state, style, derived_from_memory_ids,
                   to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from agent_self_models
            where agent_id = $1
            order by version desc
            limit 1
            "#,
        )
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(model)
    }

    /// Ensures a current default self-model exists and returns the latest model.
    pub async fn ensure_self_model(
        &self,
        agent_id: &str,
        identity: &Value,
        current_state: &Value,
        style: &Value,
    ) -> Result<StoredAgentSelfModel, StorageError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("select pg_advisory_xact_lock(hashtext($1)::bigint)")
            .bind(agent_id)
            .execute(&mut *tx)
            .await?;

        if let Some(latest) = sqlx::query_as::<_, StoredAgentSelfModel>(
            r#"
            select agent_id, version, identity, current_state, style, derived_from_memory_ids,
                   to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from agent_self_models
            where agent_id = $1
            order by version desc
            limit 1
            "#,
        )
        .bind(agent_id)
        .fetch_optional(&mut *tx)
        .await?
        {
            if latest.identity == *identity && latest.style == *style {
                tx.commit().await?;
                return Ok(latest);
            }

            let inserted = sqlx::query_as::<_, StoredAgentSelfModel>(
                r#"
                insert into agent_self_models (
                    agent_id, version, identity, current_state, style, derived_from_memory_ids
                )
                values ($1, $2, $3, $4, $5, $6)
                returning agent_id, version, identity, current_state, style, derived_from_memory_ids,
                          to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
                "#,
            )
            .bind(agent_id)
            .bind(latest.version + 1)
            .bind(identity)
            .bind(&latest.current_state)
            .bind(style)
            .bind(&latest.derived_from_memory_ids)
            .fetch_one(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(inserted);
        }

        let inserted = sqlx::query_as::<_, StoredAgentSelfModel>(
            r#"
            insert into agent_self_models (
                agent_id, version, identity, current_state, style, derived_from_memory_ids
            )
            values ($1, 1, $2, $3, $4, array[]::bigint[])
            returning agent_id, version, identity, current_state, style, derived_from_memory_ids,
                      to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            "#,
        )
        .bind(agent_id)
        .bind(identity)
        .bind(current_state)
        .bind(style)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(inserted)
    }

    /// Records a new self-model current-state version when the state changed.
    pub async fn record_self_model_state(
        &self,
        agent_id: &str,
        current_state: &Value,
    ) -> Result<StoredAgentSelfModel, StorageError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("select pg_advisory_xact_lock(hashtext($1)::bigint)")
            .bind(agent_id)
            .execute(&mut *tx)
            .await?;

        let Some(latest) = sqlx::query_as::<_, StoredAgentSelfModel>(
            r#"
            select agent_id, version, identity, current_state, style, derived_from_memory_ids,
                   to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from agent_self_models
            where agent_id = $1
            order by version desc
            limit 1
            "#,
        )
        .bind(agent_id)
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Err(StorageError::Sqlx(sqlx::Error::RowNotFound));
        };
        if latest.current_state == *current_state {
            tx.commit().await?;
            return Ok(latest);
        }

        let inserted = sqlx::query_as::<_, StoredAgentSelfModel>(
            r#"
            insert into agent_self_models (
                agent_id, version, identity, current_state, style, derived_from_memory_ids
            )
            values ($1, $2, $3, $4, $5, $6)
            returning agent_id, version, identity, current_state, style, derived_from_memory_ids,
                      to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            "#,
        )
        .bind(agent_id)
        .bind(latest.version + 1)
        .bind(&latest.identity)
        .bind(current_state)
        .bind(&latest.style)
        .bind(&latest.derived_from_memory_ids)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(inserted)
    }
}

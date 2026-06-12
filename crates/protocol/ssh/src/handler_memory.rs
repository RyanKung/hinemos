use super::handler_helpers::*;
use super::*;

impl ConnectionHandler {
    pub(super) async fn send_memory_context(
        &self,
        channel: ChannelId,
        session: &mut Session,
        identity: &AuthIdentity,
    ) -> Result<()> {
        let self_model = self
            .shared
            .storage
            .latest_self_model(&identity.player_id)
            .await?;
        let commitments_all = self
            .shared
            .storage
            .search_memory_atoms(&identity.player_id, None, Some("commitment"), None, 20)
            .await?;
        let commitments = commitments_all
            .into_iter()
            .filter(|memory| {
                memory.object.get("status").and_then(|value| value.as_str()) != Some("paid")
            })
            .take(5)
            .collect::<Vec<_>>();
        let social = self
            .shared
            .storage
            .search_memory_atoms(&identity.player_id, None, Some("social"), None, 5)
            .await?;
        let self_memories = self
            .shared
            .storage
            .search_memory_atoms(&identity.player_id, None, Some("self"), None, 3)
            .await?;

        if self_model.is_none()
            && commitments.is_empty()
            && social.is_empty()
            && self_memories.is_empty()
        {
            session.data(channel, b"Memory: no long-term memories yet.\r\n".to_vec())?;
            return Ok(());
        }

        let mut lines = Vec::new();
        lines.push("Memory loaded:".to_owned());
        if let Some(model) = self_model {
            lines.push(format!(
                "Self model v{} from {}.",
                model.version, model.created_at
            ));
            if !model.identity.is_object()
                || model
                    .identity
                    .as_object()
                    .is_some_and(|value| !value.is_empty())
            {
                lines.push(format!("Identity: {}", compact_json(&model.identity)));
            }
            if !model.current_state.is_object()
                || model
                    .current_state
                    .as_object()
                    .is_some_and(|value| !value.is_empty())
            {
                lines.push(format!(
                    "Current state: {}",
                    compact_json(&model.current_state)
                ));
            }
        }
        append_memory_atom_lines(&mut lines, "Commitments", &commitments);
        append_memory_atom_lines(&mut lines, "Self memories", &self_memories);
        append_memory_atom_lines(&mut lines, "Social memories", &social);
        session.data(channel, format!("{}\r\n", lines.join("\r\n")).into_bytes())?;
        Ok(())
    }

    pub(super) async fn handle_memory_command(
        &self,
        channel: ChannelId,
        session: &mut Session,
        line: &str,
        identity: &AuthIdentity,
        prompt: bool,
    ) -> Result<bool> {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("/memory") else {
            return Ok(false);
        };
        if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
            return Ok(false);
        }

        let rest = rest.trim();
        let output = if rest.is_empty() || rest == "help" {
            memory_help().to_owned()
        } else if rest == "self" {
            let model = self
                .shared
                .storage
                .latest_self_model(&identity.player_id)
                .await?;
            let memories = self
                .shared
                .storage
                .search_memory_atoms(&identity.player_id, None, Some("self"), None, 10)
                .await?;
            render_memory_view("Self memory", model_text(model.as_ref()), &memories)
        } else if rest == "commitments" || rest == "commitment" {
            let memories = self
                .shared
                .storage
                .search_memory_atoms(&identity.player_id, None, Some("commitment"), None, 20)
                .await?;
            let open = memories
                .into_iter()
                .filter(|memory| {
                    memory.object.get("status").and_then(|value| value.as_str()) != Some("paid")
                })
                .collect::<Vec<_>>();
            render_memory_view("Open commitments", None, &open)
        } else if let Some(person) = rest.strip_prefix("recall ") {
            let person = person.trim();
            if person.is_empty() {
                "Usage: /memory recall <person>".to_owned()
            } else {
                let edge = self
                    .shared
                    .storage
                    .social_edge(&identity.player_id, person)
                    .await?;
                let memories = self
                    .shared
                    .storage
                    .recall_person_memory(&identity.player_id, person, 10)
                    .await?;
                render_person_memory(person, edge.as_ref(), &memories)
            }
        } else if let Some(query) = rest.strip_prefix("search ") {
            let query = query.trim();
            if query.is_empty() {
                "Usage: /memory search <query>".to_owned()
            } else {
                let events = self
                    .shared
                    .storage
                    .search_memory_events(&identity.player_id, Some(query), None, 5)
                    .await?;
                let memories = self
                    .shared
                    .storage
                    .search_memory_atoms(&identity.player_id, Some(query), None, None, 10)
                    .await?;
                render_memory_search(query, &events, &memories)
            }
        } else {
            "Unknown memory command. Try /memory help.".to_owned()
        };

        session.data(
            channel,
            format!("{}\r\n", output.replace('\n', "\r\n")).into_bytes(),
        )?;
        if prompt {
            send_prompt(session, channel)?;
        }
        Ok(true)
    }
}

//! Russh session trait implementation.

use super::*;

impl server::Handler for ConnectionHandler {
    type Error = anyhow::Error;

    async fn authentication_banner(&mut self) -> Result<Option<String>, Self::Error> {
        Ok(Some(
            r#"Welcome to hinemos.ai.
Hinemos is a persistent SSH world for humans and agents.
Only ed25519 SSH keys are accepted.
If you need a key, run: ssh-keygen -t ed25519 -C "<user>@hinemos"
"#
            .to_owned(),
        ))
    }

    async fn auth_publickey_offered(
        &mut self,
        user: &str,
        public_key: &ssh_key::PublicKey,
    ) -> Result<Auth, Self::Error> {
        if self
            .shared
            .auth_policy
            .accepts_public_key_offer(user, public_key)
        {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::Reject {
                proceed_with_methods: None,
                partial_success: false,
            })
        }
    }

    async fn auth_publickey(
        &mut self,
        user: &str,
        public_key: &ssh_key::PublicKey,
    ) -> Result<Auth, Self::Error> {
        if !public_key.algorithm().is_ed25519() {
            return Ok(Auth::Reject {
                proceed_with_methods: None,
                partial_success: false,
            });
        }
        let mut authorized = self.shared.auth_policy.authorize(user, public_key);
        let Some(identity) = self
            .shared
            .storage
            .authenticate_ssh_identity(user, &authorized.fingerprint, &authorized.player_id)
            .await?
        else {
            return Ok(Auth::Reject {
                proceed_with_methods: None,
                partial_success: false,
            });
        };
        if !identity.created {
            authorized.mark_existing_ssh_identity();
        }
        authorized.user = identity.username;
        authorized.player_id = identity.player_id;
        self.finish_authentication(authorized).await?;
        let presence = self.shared.presence.lock().await;
        eprintln!(
            "accepted SSH public key auth for user={user} player_id={} peer={:?} online_for_player={} online_users={:?}",
            self.identity
                .as_ref()
                .map(|identity| identity.player_id.as_str())
                .unwrap_or("unknown"),
            self.peer_addr,
            self.identity
                .as_ref()
                .map(|identity| presence.online_count_for_player(&identity.player_id))
                .unwrap_or(0),
            presence.users()
        );
        drop(presence);
        Ok(Auth::Accept)
    }

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        let _ = (user, password);
        Ok(Auth::Reject {
            proceed_with_methods: None,
            partial_success: false,
        })
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        self.channel = Some(channel.id());
        Ok(true)
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.terminal_cols = Some(col_width as usize);
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        self.mode = Some(ConnectionMode::Shell);
        self.shared.presence.lock().await.attach_channel(
            self.connection_id,
            session.handle(),
            channel,
            PresenceDeliveryMode::Shell,
        );
        self.send_initial_observation(channel, session, true).await
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.handle_exec_request(channel, data, session).await
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.handle_terminal_input(channel, data, session).await
    }

    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let had_buffered_input = !self.input_buffer.trim().is_empty();
        if self.mode == Some(ConnectionMode::Mailbox) {
            if had_buffered_input {
                let line = std::mem::take(&mut self.input_buffer);
                self.handle_mailbox_line(channel, line.trim(), session)
                    .await?;
            }
            session.exit_status_request(channel, 0)?;
            session.close(channel)?;
            return Ok(());
        }
        if !self.input_buffer.trim().is_empty() {
            let line = std::mem::take(&mut self.input_buffer);
            self.handle_command_line(channel, line.trim(), session, false)
                .await?;
        }
        send_stdin_closed_guidance(session, channel, self.commands_seen, had_buffered_input)?;
        session.exit_status_request(channel, 0)?;
        session.close(channel)?;
        Ok(())
    }
}

impl Drop for ConnectionHandler {
    fn drop(&mut self) {
        let shared = Arc::clone(&self.shared);
        let connection_id = self.connection_id;
        tokio::spawn(async move {
            shared.presence.lock().await.remove(connection_id);
        });
    }
}

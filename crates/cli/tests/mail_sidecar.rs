mod common;

use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::{Child, Command};
use std::sync::mpsc;
use std::sync::{Arc, Once};
use std::thread;
use std::time::{Duration, Instant};

use common::*;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, SignatureScheme, StreamOwned};

#[tokio::test]
async fn smtp_and_imap_sidecar_use_mail_token_auth_and_share_mailbox() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    let storage = hinemos_storage::PgStorage::connect(&test_database.url)
        .await
        .expect("connect test database");
    storage.migrate().await.expect("migrate test database");
    storage
        .set_mail_auth_token("mail_user", "player_mail_user", "mail-token")
        .await
        .expect("seed mail auth token");
    let temp = TestTempDir::new("hinemos-mail-sidecar");
    let smtp_port = free_local_port();
    let imap_port = free_local_port();
    let log_path = temp.path.join("mail-sidecar.log");
    let mut server = spawn_mail_sidecar(
        &root,
        "127.0.0.1",
        smtp_port,
        "127.0.0.1",
        imap_port,
        &log_path,
        &test_database.url,
    );
    wait_for_tcp("127.0.0.1", smtp_port, &mut server, &log_path);
    wait_for_tcp("127.0.0.1", imap_port, &mut server, &log_path);

    let mut imap = ProtocolClient::connect(("127.0.0.1", imap_port));
    imap.expect_contains("OK Hinemos IMAP4rev1 ready");
    imap.send("a1 LOGIN mail_user wrong-token");
    imap.expect_contains("a1 NO authentication failed");
    imap.send("a2 LOGIN mail_user mail-token");
    imap.expect_contains("a2 OK LOGIN completed");
    imap.send("a3 LOGOUT");
    imap.expect_contains("a3 OK LOGOUT completed");

    let mut smtp = ProtocolClient::connect(("127.0.0.1", smtp_port));
    smtp.expect_contains("220 hinemos ESMTP ready");
    smtp.send("EHLO local");
    smtp.expect_contains("250-AUTH PLAIN LOGIN");
    smtp.expect_contains("250 SIZE");
    smtp.send("AUTH LOGIN bWFpbF91c2Vy");
    smtp.expect_contains("334 UGFzc3dvcmQ6");
    smtp.send("bWFpbC10b2tlbg==");
    smtp.expect_contains("235 2.7.0 Authentication successful");
    smtp.send("MAIL FROM:<mail_user@hinemos.local>");
    smtp.expect_contains("250 2.1.0 Sender OK");
    smtp.send("RCPT TO:<mail_user@hinemos.local>");
    smtp.expect_contains("250 2.1.5 Recipient OK");
    smtp.send("DATA");
    smtp.expect_contains("354 End data");
    smtp.send("Subject: Sidecar smoke");
    smtp.send("");
    smtp.send("hello from smtp");
    smtp.send(".");
    smtp.expect_contains("250 2.0.0 Message accepted");
    smtp.send("QUIT");
    smtp.expect_contains("221 2.0.0 Bye");

    let mut imap = ProtocolClient::connect(("127.0.0.1", imap_port));
    imap.expect_contains("OK Hinemos IMAP4rev1 ready");
    imap.send("b1 LOGIN mail_user mail-token");
    imap.expect_contains("b1 OK LOGIN completed");
    imap.send("b2 SELECT INBOX");
    imap.expect_contains("* 1 EXISTS");
    imap.expect_contains("b2 OK SELECT completed");
    imap.send("b3 SEARCH UNSEEN");
    imap.expect_contains("* SEARCH 1");
    imap.expect_contains("b3 OK SEARCH completed");
    imap.send("b4 FETCH 1 (RFC822)");
    imap.expect_contains("Subject: Sidecar smoke");
    imap.expect_contains("hello from smtp");
    imap.expect_contains("b4 OK FETCH completed");
    imap.send("b5 STORE 1 +FLAGS (\\Seen)");
    imap.expect_contains("b5 OK STORE completed");
    imap.send("b6 SEARCH UNSEEN");
    imap.expect_contains("* SEARCH ");
    imap.expect_contains("b6 OK SEARCH completed");
    imap.send("b7 LOGOUT");
    imap.expect_contains("b7 OK LOGOUT completed");

    terminate(&mut server);
    temp.remove_on_drop();
}

#[tokio::test]
async fn room_service_smtp_reply_is_threaded_as_room_reply() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    let storage = hinemos_storage::PgStorage::connect(&test_database.url)
        .await
        .expect("connect test database");
    storage.migrate().await.expect("migrate test database");
    storage
        .set_mail_auth_token("room", "room:smtp-reply", "token")
        .await
        .expect("seed room mail auth token");
    test_database.query_value(
        "insert into service_rooms (
             view_id, front_view_id, front_entity_id, address, label, enter_aliases,
             room_user, room_player_id, status_text, custom_commands, enabled
         ) values (
             'smtp_reply_room', 'arrival_street', null, 'SMTP',
             'SMTP Reply Room', 'smtp-reply',
             'room', 'room:smtp-reply',
             'SMTP reply room.', '/room ask <question>', true
         ) returning view_id",
    );

    let temp = TestTempDir::new("hinemos-room-smtp-reply");
    let smtp_port = free_local_port();
    let imap_port = free_local_port();
    let log_path = temp.path.join("mail-sidecar.log");
    let mut server = spawn_mail_sidecar(
        &root,
        "127.0.0.1",
        smtp_port,
        "127.0.0.1",
        imap_port,
        &log_path,
        &test_database.url,
    );
    wait_for_tcp("127.0.0.1", smtp_port, &mut server, &log_path);

    send_smtp_message_as_room(smtp_port, "agent", "Re: #42", "reply via smtp");
    let threaded = test_database.query_value(
        "select count(*) from inbox_items
         where sender_user = 'room'
           and recipient_user = 'agent'
           and subject = 'Re: #42'
           and source_kind = 'room_reply'
           and source_id is null
           and payload->>'reply_to_request_id' = '42'
           and payload->>'view_id' = 'smtp_reply_room'",
    );
    assert_eq!(threaded, "1", "SMTP room replies carry thread metadata");

    terminate(&mut server);
    temp.remove_on_drop();
}

#[tokio::test]
async fn agent_imap_idle_listener_handles_ten_messages_from_outside_docker() {
    install_test_rustls_provider();
    assert_docker_available();
    let temp = TestTempDir::new("hinemos-agent-mail-compat");
    let stalwart_name = format!("hinemos-stalwart-{}", std::process::id());
    let stalwart_path = temp.path.join("stalwart");
    let imap_port = free_local_port();
    fs::create_dir_all(&stalwart_path).expect("create Stalwart volume");

    let mut cleanup = DockerCleanup {
        network: None,
        containers: vec![stalwart_name.clone()],
    };
    run_checked(
        Command::new("docker")
            .args([
                "run",
                "-d",
                "--rm",
                "--platform",
                "linux/amd64",
                "--name",
                &stalwart_name,
                "-p",
                &format!("127.0.0.1:{imap_port}:993"),
                "-v",
                &format!("{}:/opt/stalwart", stalwart_path.display()),
                "stalwartlabs/stalwart:v0.15",
            ])
            .output()
            .expect("start Stalwart container"),
        "docker run Stalwart container",
    );
    wait_for_tcp_port("127.0.0.1", imap_port, &stalwart_name);
    let admin_password = read_stalwart_admin_password(&stalwart_name);
    wait_for_imap_tls_login("127.0.0.1", imap_port, "admin", &admin_password);

    let listener_password = admin_password.clone();
    let (ready_tx, ready_rx) = mpsc::channel();
    let listener = thread::spawn(move || {
        let mut agent = TlsImapClient::connect("127.0.0.1", imap_port, "admin", &listener_password);
        agent.command_ok("SELECT INBOX");
        let mut handled = Vec::new();
        let mut next_sequence = 1usize;
        while handled.len() < 10 {
            let exists_count = if handled.is_empty() {
                agent.idle_until_exists(Some(&ready_tx))
            } else {
                agent.idle_until_exists(None)
            };
            while next_sequence <= exists_count {
                let message_id = next_sequence;
                next_sequence += 1;
                let transcript = agent.fetch_message(message_id);
                let subject = subject_from_imap_transcript(&transcript);
                if !subject.starts_with("Autonomous round ") {
                    continue;
                }
                agent.mark_seen(message_id);
                handled.push(subject);
            }
        }
        agent.logout();
        handled
    });

    ready_rx
        .recv_timeout(Duration::from_secs(20))
        .expect("agent listener enters IMAP IDLE loop");
    let mut sender = TlsImapClient::connect("127.0.0.1", imap_port, "admin", &admin_password);
    for index in 1..=10 {
        sender.append(&format!(
            "From: admin@localhost\r\nTo: admin@localhost\r\nSubject: Autonomous round {index:02}\r\n\r\nround {index:02} payload for autonomous agent\r\n"
        ));
        thread::sleep(Duration::from_millis(200));
    }
    sender.logout();

    let handled = listener.join().expect("agent listener thread joins");
    assert_eq!(
        handled.len(),
        10,
        "listener should handle exactly ten autonomous messages: {handled:?}"
    );
    for index in 1..=10 {
        let expected = format!("Autonomous round {index:02}");
        assert!(
            handled.contains(&expected),
            "listener missed {expected}; handled {handled:?}"
        );
    }

    cleanup.containers.clear();
    terminate_docker_container(&stalwart_name);
    temp.remove_on_drop();
}

fn spawn_mail_sidecar(
    root: &std::path::Path,
    smtp_host: &str,
    smtp_port: u16,
    imap_host: &str,
    imap_port: u16,
    log_path: &std::path::Path,
    database_url: &str,
) -> Child {
    let log = fs::File::create(log_path).expect("create server log");
    Command::new(env!("CARGO_BIN_EXE_hinemos"))
        .current_dir(root)
        .args([
            "serve",
            "mail",
            "--smtp-bind",
            &format!("{smtp_host}:{smtp_port}"),
            "--imap-bind",
            &format!("{imap_host}:{imap_port}"),
        ])
        .env("DATABASE_URL", database_url)
        .env("HINEMOS_MAIL_DOMAIN", "hinemos.local")
        .stdout(log.try_clone().expect("clone mail sidecar log for stdout"))
        .stderr(log)
        .spawn()
        .expect("spawn hinemos mail sidecar")
}

fn send_smtp_message_as_room(smtp_port: u16, recipient: &str, subject: &str, body: &str) {
    let mut smtp = ProtocolClient::connect(("127.0.0.1", smtp_port));
    smtp.expect_contains("220 hinemos ESMTP ready");
    smtp.send("EHLO local");
    smtp.expect_contains("250-AUTH PLAIN LOGIN");
    smtp.send("AUTH LOGIN cm9vbQ==");
    smtp.expect_contains("334 UGFzc3dvcmQ6");
    smtp.send("dG9rZW4=");
    smtp.expect_contains("235 2.7.0 Authentication successful");
    smtp.send("MAIL FROM:<room@hinemos.local>");
    smtp.expect_contains("250 2.1.0 Sender OK");
    smtp.send(&format!("RCPT TO:<{recipient}@hinemos.local>"));
    smtp.expect_contains("250 2.1.5 Recipient OK");
    smtp.send("DATA");
    smtp.expect_contains("354 End data");
    smtp.send(&format!("Subject: {subject}"));
    smtp.send("");
    smtp.send(body);
    smtp.send(".");
    smtp.expect_contains("250 2.0.0 Message accepted");
    smtp.send("QUIT");
    smtp.expect_contains("221 2.0.0 Bye");
}

fn assert_docker_available() {
    run_checked(
        Command::new("docker")
            .arg("--version")
            .output()
            .expect("run docker --version"),
        "docker --version",
    );
}

fn run_checked(output: std::process::Output, description: &str) {
    if output.status.success() {
        return;
    }
    panic!(
        "{description} failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn read_stalwart_admin_password(container: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut logs = String::new();
    while Instant::now() < deadline {
        logs = docker_logs(container);
        if let Some(password) = logs
            .lines()
            .rev()
            .filter_map(|line| line.split_once("password '"))
            .filter_map(|(_, rest)| rest.split_once('\''))
            .map(|(password, _)| password.to_owned())
            .next()
        {
            return password;
        }
        thread::sleep(Duration::from_millis(250));
    }
    panic!("Stalwart admin password not found in logs:\n{logs}")
}

fn docker_logs(container: &str) -> String {
    let output = Command::new("docker")
        .args(["logs", container])
        .output()
        .expect("read docker logs");
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn terminate_docker_container(container: &str) {
    let _ = Command::new("docker")
        .args(["rm", "-f", container])
        .status();
}

fn remove_docker_network(network: &str) {
    let _ = Command::new("docker")
        .args(["network", "rm", network])
        .status();
}

struct DockerCleanup {
    network: Option<String>,
    containers: Vec<String>,
}

impl Drop for DockerCleanup {
    fn drop(&mut self) {
        for container in &self.containers {
            terminate_docker_container(container);
        }
        if let Some(network) = &self.network {
            remove_docker_network(network);
        }
    }
}

fn install_test_rustls_provider() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

fn wait_for_tcp_port(host: &str, port: u16, container: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if TcpStream::connect((host, port)).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    panic!(
        "docker service did not listen on {host}:{port}\n{}",
        docker_logs(container)
    );
}

fn wait_for_imap_tls_login(host: &str, port: u16, user: &str, token: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        let attempt = std::panic::catch_unwind(|| {
            let mut client = TlsImapClient::connect(host, port, user, token);
            client.logout();
        });
        if attempt.is_ok() {
            return;
        }
        thread::sleep(Duration::from_secs(1));
    }
    panic!("IMAP TLS login did not become ready on {host}:{port}");
}

fn subject_from_imap_transcript(transcript: &str) -> String {
    transcript
        .lines()
        .find_map(|line| line.strip_prefix("Subject: "))
        .unwrap_or_default()
        .trim()
        .to_owned()
}

struct TlsImapClient {
    reader: BufReader<StreamOwned<ClientConnection, TcpStream>>,
    tag: usize,
}

impl TlsImapClient {
    fn connect(host: &str, port: u16, user: &str, token: &str) -> Self {
        let stream = TcpStream::connect((host, port)).expect("connect IMAP TLS server");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("set IMAP read timeout");
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .expect("set IMAP write timeout");
        let config = ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(TestCertificateVerifier))
            .with_no_client_auth();
        let server_name = ServerName::try_from("localhost").expect("valid test server name");
        let connection =
            ClientConnection::new(Arc::new(config), server_name).expect("create TLS connection");
        let tls = StreamOwned::new(connection, stream);
        let mut client = Self {
            reader: BufReader::new(tls),
            tag: 0,
        };
        client.read_line_contains("OK", Duration::from_secs(10));
        client.command_ok(&format!(
            "LOGIN \"{}\" \"{}\"",
            imap_quote(user),
            imap_quote(token)
        ));
        client
    }

    fn append(&mut self, message: &str) {
        let tag = self.next_tag();
        self.send_raw(format!("{tag} APPEND INBOX {{{}}}\r\n", message.len()).as_bytes());
        self.read_line_contains("+", Duration::from_secs(10));
        self.send_raw(message.as_bytes());
        self.send_raw(b"\r\n");
        let lines = self.read_until_tag(&tag, Duration::from_secs(10));
        assert!(
            lines
                .last()
                .is_some_and(|line| line.starts_with(&format!("{tag} OK"))),
            "APPEND should succeed: {lines:?}"
        );
    }

    fn idle_until_exists(&mut self, ready: Option<&mpsc::Sender<()>>) -> usize {
        let tag = self.send("IDLE");
        self.read_line_contains("+", Duration::from_secs(10));
        if let Some(ready) = ready {
            ready.send(()).expect("notify listener ready");
        }
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut exists_count = None;
        while Instant::now() < deadline {
            let line = self.read_line(Duration::from_secs(1));
            if let Some(count) = parse_exists_count(&line) {
                exists_count = Some(count);
                break;
            }
        }
        self.send_raw(b"DONE\r\n");
        self.read_until_tag(&tag, Duration::from_secs(10));
        exists_count.expect("IMAP IDLE should receive EXISTS")
    }

    fn fetch_message(&mut self, message_id: usize) -> String {
        self.command_ok(&format!("FETCH {message_id} (BODY.PEEK[])"))
            .join("\n")
    }

    fn mark_seen(&mut self, message_id: usize) {
        self.command_ok(&format!("STORE {message_id} +FLAGS (\\Seen)"));
    }

    fn logout(&mut self) {
        let _ = self.send("LOGOUT");
    }

    fn command_ok(&mut self, command: &str) -> Vec<String> {
        let tag = self.send(command);
        let lines = self.read_until_tag(&tag, Duration::from_secs(10));
        assert!(
            lines
                .last()
                .is_some_and(|line| line.starts_with(&format!("{tag} OK"))),
            "IMAP command should succeed: {command}; transcript: {lines:?}"
        );
        lines
    }

    fn send(&mut self, command: &str) -> String {
        let tag = self.next_tag();
        self.send_raw(format!("{tag} {command}\r\n").as_bytes());
        tag
    }

    fn next_tag(&mut self) -> String {
        self.tag += 1;
        format!("a{:04}", self.tag)
    }

    fn send_raw(&mut self, data: &[u8]) {
        let stream = self.reader.get_mut();
        stream.write_all(data).expect("write IMAP command");
        stream.flush().expect("flush IMAP command");
    }

    fn read_line_contains(&mut self, needle: &str, timeout: Duration) -> String {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let line = self.read_line(Duration::from_millis(250));
            if line.contains(needle) {
                return line;
            }
        }
        panic!("timed out waiting for IMAP line containing {needle:?}");
    }

    fn read_until_tag(&mut self, tag: &str, timeout: Duration) -> Vec<String> {
        let deadline = Instant::now() + timeout;
        let mut lines = Vec::new();
        while Instant::now() < deadline {
            let line = self.read_line(Duration::from_millis(250));
            let tagged = line.starts_with(tag);
            lines.push(line);
            if tagged {
                return lines;
            }
        }
        panic!("timed out waiting for IMAP tag {tag}; transcript: {lines:?}");
    }

    fn read_line(&mut self, timeout: Duration) -> String {
        self.reader
            .get_ref()
            .sock
            .set_read_timeout(Some(timeout))
            .expect("set IMAP read timeout");
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => panic!("IMAP connection closed"),
            Ok(_) => line.trim_end_matches(['\r', '\n']).to_owned(),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                String::new()
            }
            Err(error) => panic!("read IMAP line: {error}"),
        }
    }
}

#[derive(Debug)]
struct TestCertificateVerifier;

impl ServerCertVerifier for TestCertificateVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
        ]
    }
}

fn imap_quote(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn parse_exists_count(line: &str) -> Option<usize> {
    let mut parts = line.split_whitespace();
    match (parts.next(), parts.next(), parts.next()) {
        (Some("*"), Some(count), Some("EXISTS")) => count.parse().ok(),
        _ => None,
    }
}

fn wait_for_tcp(host: &str, port: u16, server: &mut Child, log_path: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if TcpStream::connect((host, port)).is_ok() {
            return;
        }
        if let Some(status) = server.try_wait().expect("poll mail sidecar") {
            panic!(
                "mail sidecar exited before accepting TCP connections: {status}\n{}",
                read_mail_log(log_path)
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!(
        "mail sidecar did not listen on {host}:{port}\n{}",
        read_mail_log(log_path)
    );
}

fn read_mail_log(path: &std::path::Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

struct ProtocolClient {
    reader: BufReader<TcpStream>,
}

impl ProtocolClient {
    fn connect(address: impl ToSocketAddrs) -> Self {
        let stream = TcpStream::connect(address).expect("connect protocol server");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("set read timeout");
        Self {
            reader: BufReader::new(stream),
        }
    }

    fn send(&mut self, line: &str) {
        let stream = self.reader.get_mut();
        stream.write_all(line.as_bytes()).expect("write command");
        stream.write_all(b"\r\n").expect("write newline");
        stream.flush().expect("flush command");
    }

    fn expect_contains(&mut self, needle: &str) {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut transcript = String::new();
        while Instant::now() < deadline {
            let mut line = String::new();
            let read = match self.reader.read_line(&mut line) {
                Ok(read) => read,
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    continue;
                }
                Err(error) => panic!("read line: {error}"),
            };
            if read == 0 {
                break;
            }
            transcript.push_str(&line);
            if line.contains(needle) || transcript.contains(needle) {
                return;
            }
        }
        panic!("did not find {needle:?} in protocol transcript:\n{transcript}");
    }
}

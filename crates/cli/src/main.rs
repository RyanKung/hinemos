#![deny(missing_docs)]

//! Local stdin/stdout CLI adapter for the Hinemos open-world runtime.

use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use hinemos_admin_protocol::{AdminRequest, AdminResponse};
use hinemos_core::sample_world::{LOCAL_PLAYER_ID, load_world_from_dir};
use hinemos_core::{JsonObservation, SemanticCommand};
use hinemos_runtime::{Chrome, GameRuntime, render_text_observation};
use hinemos_storage::{NewMemoryAtom, NewMemoryEvent, PgStorage};
use serde_json::json;

mod rooms_service;

#[derive(Debug, Parser)]
#[command(name = "hinemos")]
#[command(about = "A local open-world prototype for agent exploration")]
struct Cli {
    #[cfg(unix)]
    #[command(subcommand)]
    sub: Option<TopCommand>,

    #[command(flatten)]
    play: PlayArgs,
}

#[cfg(unix)]
#[derive(Debug, Subcommand)]
enum TopCommand {
    /// Control a running daemon via its admin Unix socket.
    Admin(AdminCli),
    /// Store and recall agent memory.
    Memory(MemoryCli),
    /// Run a network adapter.
    #[command(subcommand)]
    Serve(ServeCli),
}

#[cfg(unix)]
#[derive(Debug, Subcommand)]
enum ServeCli {
    /// Run the HTTP adapter.
    Http(hinemos_http::HttpArgs),
    /// Run the SSH adapter.
    Ssh(hinemos_ssh::SshArgs),
    /// Run the SMTP/IMAP mail sidecar.
    Mail(hinemos_ssh::MailArgs),
    /// Run the built-in external room service workers.
    Rooms(rooms_service::RoomsArgs),
}

#[derive(Debug, Parser)]
struct PlayArgs {
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,

    #[arg(long, default_value = "worlds/sample")]
    world: PathBuf,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Parser)]
struct AdminCli {
    #[arg(long, default_value = ".hinemos/admin.sock")]
    socket: PathBuf,

    #[command(subcommand)]
    cmd: AdminCmd,
}

#[derive(Debug, Parser)]
struct MemoryCli {
    #[arg(long)]
    database_url: Option<String>,

    #[command(subcommand)]
    cmd: MemoryCmd,
}

#[derive(Debug, Subcommand)]
enum MemoryCmd {
    /// Append an event and create a default semantic memory atom.
    Remember {
        #[arg(long)]
        agent: String,
        #[arg(long, default_value = "manual")]
        source: String,
        #[arg(long = "type", default_value = "manual")]
        event_type: String,
        #[arg(long = "actor")]
        actors: Vec<String>,
        #[arg(long, default_value_t = 0.5)]
        salience: f64,
        #[arg(long, default_value = "episodic")]
        kind: String,
        #[arg(long)]
        subject: Option<String>,
        #[arg(long, default_value = "observed")]
        predicate: String,
        #[arg(required = true)]
        content: Vec<String>,
    },
    /// Search semantic memories.
    Search {
        #[arg(long)]
        agent: String,
        #[arg(long = "type")]
        event_type: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        subject: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: i64,
        query: Vec<String>,
    },
    /// Recall self or person-specific memory.
    Recall {
        #[arg(long)]
        agent: String,
        #[arg(long)]
        person: Option<String>,
        #[arg(long)]
        self_model: bool,
        #[arg(long, default_value_t = 20)]
        limit: i64,
    },
    /// Print the latest self-model snapshot.
    Profile {
        #[arg(long)]
        agent: String,
    },
}

#[derive(Debug, Subcommand)]
enum AdminCmd {
    /// Verify the daemon admin socket is accepting RPCs.
    Ping,
    /// Print runtime and loaded-map summary.
    Status,
    /// Print authenticated sessions (connection id, player id, SSH user).
    Sessions,
    /// Print online SSH users grouped across sessions.
    Users,
    /// Ask the daemon to close an SSH session after its next input chunk.
    Kick {
        /// Value from `sessions` output.
        connection_id: u64,
    },
    /// Reload world RON files from disk (player positions preserved when possible).
    ReloadWorld {
        #[arg(long)]
        world: Option<PathBuf>,
    },
    /// Reload map RON files from disk (alias for `reload-world`).
    ReloadMap {
        #[arg(long)]
        world: Option<PathBuf>,
    },
    /// Generate or rotate SMTP/IMAP token for an externally registered service room.
    RoomToken {
        /// Service room view id.
        view_id: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    #[cfg(unix)]
    if let Some(sub) = cli.sub {
        return match sub {
            TopCommand::Admin(admin) => run_admin(admin),
            TopCommand::Memory(memory) => run_memory(memory).await,
            TopCommand::Serve(ServeCli::Http(args)) => hinemos_http::run_daemon(args).await,
            TopCommand::Serve(ServeCli::Ssh(args)) => hinemos_ssh::run_daemon(args).await,
            TopCommand::Serve(ServeCli::Mail(args)) => hinemos_ssh::run_mail_daemon(args).await,
            TopCommand::Serve(ServeCli::Rooms(args)) => rooms_service::run(args).await,
        };
    }

    run_play(cli.play)
}

#[cfg(unix)]
fn run_admin(admin: AdminCli) -> Result<()> {
    use hinemos_admin_protocol::unix_admin_call;

    let request = match admin.cmd {
        AdminCmd::Ping => AdminRequest::Ping,
        AdminCmd::Status => AdminRequest::Status,
        AdminCmd::Sessions => AdminRequest::ListSessions,
        AdminCmd::Users => AdminRequest::ListUsers,
        AdminCmd::Kick { connection_id } => AdminRequest::KickConnection { connection_id },
        AdminCmd::ReloadWorld { world } => AdminRequest::ReloadWorld { world_dir: world },
        AdminCmd::ReloadMap { world } => AdminRequest::ReloadWorld { world_dir: world },
        AdminCmd::RoomToken { view_id } => AdminRequest::RoomToken { view_id },
    };

    let response = unix_admin_call(&admin.socket, &request)?;

    match response {
        AdminResponse::Pong => println!("pong"),
        AdminResponse::Ok { message } => println!("{message}"),
        AdminResponse::Status { summary } => {
            println!(
                "sessions={} users={} views={} entities={} players={}",
                summary.session_count,
                summary.user_count,
                summary.view_count,
                summary.entity_count,
                summary.player_count
            );
        }
        AdminResponse::Sessions { sessions } => {
            if sessions.is_empty() {
                println!("(no sessions)");
            }
            for session in sessions {
                println!(
                    "{} player={} user={}",
                    session.connection_id, session.player_id, session.user
                );
            }
        }
        AdminResponse::Users { users } => {
            if users.is_empty() {
                println!("(no users)");
            }
            for user in users {
                println!(
                    "{} sessions={} players={}",
                    user.user,
                    user.session_count,
                    user.player_ids.join(",")
                );
            }
        }
        AdminResponse::RoomToken {
            view_id,
            username,
            player_id,
            token,
        } => {
            println!("view={view_id}");
            println!("username={username}");
            println!("player={player_id}");
            println!("token={token}");
        }
        AdminResponse::Error { message } => {
            anyhow::bail!("{message}");
        }
    }

    Ok(())
}

fn run_play(play: PlayArgs) -> Result<()> {
    let world = load_world_from_dir(&play.world)
        .with_context(|| format!("failed to load world from {}", play.world.display()))?;
    let chrome = Chrome::with_world(&world);
    let runtime = GameRuntime::new(world);

    let mut current = runtime.observe_json(LOCAL_PLAYER_ID, Vec::new())?;
    print_observation(&current, play.format)?;

    let stdin = io::stdin();
    loop {
        if matches!(play.format, OutputFormat::Text) {
            print!("{}", Chrome::PROMPT);
            io::stdout().flush().context("failed to flush prompt")?;
        }

        let mut input = String::new();
        let read = stdin
            .read_line(&mut input)
            .context("failed to read command")?;
        if read == 0 {
            break;
        }

        let command = match chrome.parse_command_with_observation(&input, Some(&current)) {
            Ok(command) => command,
            Err(error) => {
                eprintln!("{error}");
                continue;
            }
        };

        let should_quit = matches!(command, SemanticCommand::Quit);
        current = runtime.execute(LOCAL_PLAYER_ID, &command)?;
        print_observation(&current, play.format)?;

        if should_quit {
            break;
        }
    }

    Ok(())
}

async fn run_memory(memory: MemoryCli) -> Result<()> {
    let database_url = memory
        .database_url
        .or_else(|| std::env::var("DATABASE_URL").ok())
        .context("DATABASE_URL must be set or passed with --database-url")?;
    let storage = PgStorage::connect(&database_url).await?;
    storage.migrate().await?;

    match memory.cmd {
        MemoryCmd::Remember {
            agent,
            source,
            event_type,
            actors,
            salience,
            kind,
            subject,
            predicate,
            content,
        } => {
            let content = content.join(" ");
            let actors_json = if actors.is_empty() {
                json!([agent])
            } else {
                json!(actors)
            };
            let event = storage
                .append_memory_event(NewMemoryEvent {
                    agent_id: agent.clone(),
                    source,
                    event_type,
                    actors: actors_json,
                    content: content.clone(),
                    world_refs: json!({}),
                    salience,
                })
                .await?;
            let subject = subject.unwrap_or_else(|| agent.clone());
            let atom = storage
                .upsert_memory_atom(NewMemoryAtom {
                    agent_id: agent.clone(),
                    kind: kind.clone(),
                    subject: subject.clone(),
                    predicate,
                    object: json!({ "content": content }),
                    summary: event.content.clone(),
                    evidence_event_ids: vec![event.id],
                    confidence: salience,
                    importance: salience,
                    emotional_valence: 0.0,
                })
                .await?;

            if kind == "social" && subject != agent {
                let _edge = storage
                    .touch_social_edge(&agent, &subject, atom.id, Some("remembered"))
                    .await?;
            }

            println!(
                "{}",
                serde_json::to_string_pretty(&json!({ "event": event, "atom": atom }))?
            );
        }
        MemoryCmd::Search {
            agent,
            event_type,
            kind,
            subject,
            limit,
            query,
        } => {
            run_memory_search(&storage, agent, event_type, kind, subject, limit, query).await?;
        }
        MemoryCmd::Recall {
            agent,
            person,
            self_model,
            limit,
        } => {
            run_memory_recall(&storage, agent, person, self_model, limit).await?;
        }
        MemoryCmd::Profile { agent } => {
            let model = storage.latest_self_model(&agent).await?;
            println!("{}", serde_json::to_string_pretty(&model)?);
        }
    }

    Ok(())
}

async fn run_memory_search(
    storage: &PgStorage,
    agent: String,
    event_type: Option<String>,
    kind: Option<String>,
    subject: Option<String>,
    limit: i64,
    query: Vec<String>,
) -> Result<()> {
    let query = (!query.is_empty()).then(|| query.join(" "));
    let atoms = storage
        .search_memory_atoms(
            &agent,
            query.as_deref(),
            kind.as_deref(),
            subject.as_deref(),
            limit,
        )
        .await?;
    let events = storage
        .search_memory_events(&agent, query.as_deref(), event_type.as_deref(), limit)
        .await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({ "events": events, "memories": atoms }))?
    );
    Ok(())
}

async fn run_memory_recall(
    storage: &PgStorage,
    agent: String,
    person: Option<String>,
    self_model: bool,
    limit: i64,
) -> Result<()> {
    if let Some(person) = person {
        let edge = storage.social_edge(&agent, &person).await?;
        let atoms = storage.recall_person_memory(&agent, &person, limit).await?;
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "edge": edge, "memories": atoms }))?
        );
    } else if self_model {
        let model = storage.latest_self_model(&agent).await?;
        let atoms = storage
            .search_memory_atoms(&agent, None, Some("self"), None, limit)
            .await?;
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "self_model": model, "memories": atoms }))?
        );
    } else {
        let atoms = storage
            .search_memory_atoms(&agent, None, None, None, limit)
            .await?;
        println!("{}", serde_json::to_string_pretty(&atoms)?);
    }
    Ok(())
}

fn print_observation(observation: &JsonObservation, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Text => {
            print!("{}", render_text_observation(observation));
            Ok(())
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(observation)?);
            Ok(())
        }
    }
}

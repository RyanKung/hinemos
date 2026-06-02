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
    /// Run a network adapter.
    #[command(subcommand)]
    Serve(ServeCli),
}

#[cfg(unix)]
#[derive(Debug, Subcommand)]
enum ServeCli {
    /// Run the SSH adapter.
    Ssh(hinemos_ssh::SshArgs),
    /// Run the SMTP/IMAP mail sidecar.
    Mail(hinemos_ssh::MailArgs),
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    #[cfg(unix)]
    if let Some(sub) = cli.sub {
        return match sub {
            TopCommand::Admin(admin) => run_admin(admin),
            TopCommand::Serve(ServeCli::Ssh(args)) => hinemos_ssh::run_daemon(args).await,
            TopCommand::Serve(ServeCli::Mail(args)) => hinemos_ssh::run_mail_daemon(args).await,
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

    let initial = runtime.observe_json(LOCAL_PLAYER_ID, Vec::new())?;
    print_observation(&initial, play.format)?;

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

        let command = match chrome.parse_command(&input) {
            Ok(command) => command,
            Err(error) => {
                eprintln!("{error}");
                continue;
            }
        };

        let should_quit = matches!(command, SemanticCommand::Quit);
        let observation = runtime.execute(LOCAL_PLAYER_ID, &command)?;
        print_observation(&observation, play.format)?;

        if should_quit {
            break;
        }
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

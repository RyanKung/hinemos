#![deny(missing_docs)]

//! Local stdin/stdout CLI adapter for the Xagora MUD runtime.

use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use xagora_core::sample_world::{LOCAL_PLAYER_ID, load_world_from_dir};
use xagora_core::{JsonObservation, ObservationEvent, SemanticCommand};
use xagora_runtime::{AdminRequest, AdminResponse, Chrome, GameRuntime};

#[derive(Debug, Parser)]
#[command(name = "xagora")]
#[command(about = "A local MUD prototype for agent exploration")]
struct Cli {
    #[cfg(unix)]
    #[command(subcommand)]
    sub: Option<AdminTop>,

    #[command(flatten)]
    play: PlayArgs,
}

#[cfg(unix)]
#[derive(Debug, Subcommand)]
enum AdminTop {
    /// Control a running daemon via its admin Unix socket.
    Admin(AdminCli),
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
    #[arg(long, default_value = ".xagora/admin.sock")]
    socket: PathBuf,

    #[command(subcommand)]
    cmd: AdminCmd,
}

#[derive(Debug, Subcommand)]
enum AdminCmd {
    /// Verify the daemon admin socket is accepting RPCs.
    Ping,
    /// Print authenticated sessions (connection id, player id, SSH user).
    Sessions,
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    #[cfg(unix)]
    if let Some(AdminTop::Admin(admin)) = cli.sub {
        return run_admin(admin);
    }

    run_play(cli.play)
}

#[cfg(unix)]
fn run_admin(admin: AdminCli) -> Result<()> {
    use xagora_runtime::unix_admin_call;

    let request = match admin.cmd {
        AdminCmd::Ping => AdminRequest::Ping,
        AdminCmd::Sessions => AdminRequest::ListSessions,
        AdminCmd::Kick { connection_id } => AdminRequest::KickConnection { connection_id },
        AdminCmd::ReloadWorld { world } => AdminRequest::ReloadWorld { world_dir: world },
    };

    let response = unix_admin_call(&admin.socket, &request)?;

    match response {
        AdminResponse::Pong => println!("pong"),
        AdminResponse::Ok { message } => println!("{message}"),
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
            print_text_observation(observation);
            Ok(())
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(observation)?);
            Ok(())
        }
    }
}

fn print_text_observation(observation: &JsonObservation) {
    println!();
    println!("{}", observation.title);
    if !observation.ascii_art.is_empty() {
        println!();
        for line in &observation.ascii_art {
            println!("{line}");
        }
    }
    println!();
    println!("{}", observation.description);

    if !observation.exits.is_empty() {
        let exits = observation
            .exits
            .iter()
            .map(|exit| exit.direction.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        println!("{}: {exits}", Chrome::LABEL_EXITS);
    }

    if !observation.entities.is_empty() {
        let entities = observation
            .entities
            .iter()
            .map(|entity| entity.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        println!("{}: {entities}", Chrome::LABEL_VISIBLE);
    }

    for event in &observation.events {
        match event {
            ObservationEvent::Message { text } => println!("{text}"),
            ObservationEvent::Move { direction, .. } => {
                println!("{} {}", Chrome::MOVE_VERB, direction.as_str());
            }
        }
    }
}

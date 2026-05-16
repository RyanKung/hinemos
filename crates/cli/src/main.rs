#![deny(missing_docs)]

//! Local stdin/stdout CLI adapter for the Agentopia MUD runtime.

use std::io::{self, Write};
use std::path::PathBuf;
use std::str::FromStr;

use agentopia_core::sample_world::{LOCAL_PLAYER_ID, load_world_from_dir};
use agentopia_core::{JsonObservation, ObservationEvent, SemanticCommand};
use agentopia_i18n::{Catalog, Language, parse_language_command};
use agentopia_runtime::{GameRuntime, Localizer};
use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "agentopia")]
#[command(about = "A local MUD prototype for agent exploration")]
struct Cli {
    #[arg(long, default_value = "en-US")]
    lang: String,

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

fn main() -> Result<()> {
    let cli = Cli::parse();
    let language = Language::from_str(&cli.lang)?;
    let mut catalog = Catalog::new(language);
    let world = load_world_from_dir(&cli.world)
        .with_context(|| format!("failed to load world from {}", cli.world.display()))?;
    let runtime = GameRuntime::new(world);

    let initial = runtime.observe_json(LOCAL_PLAYER_ID, &catalog, Vec::new())?;
    print_observation(&initial, &catalog, cli.format)?;

    let stdin = io::stdin();
    loop {
        if matches!(cli.format, OutputFormat::Text) {
            print!("{}", catalog.text("prompt"));
            io::stdout().flush().context("failed to flush prompt")?;
        }

        let mut input = String::new();
        let read = stdin
            .read_line(&mut input)
            .context("failed to read command")?;
        if read == 0 {
            break;
        }

        if let Some(language) = parse_language_command(&input) {
            match language {
                Ok(language) => {
                    catalog = Catalog::new(language);
                    let observation =
                        runtime.observe_json(LOCAL_PLAYER_ID, &catalog, Vec::new())?;
                    println!("{}", catalog.text("event.lang"));
                    print_observation(&observation, &catalog, cli.format)?;
                }
                Err(error) => eprintln!("{error}"),
            }
            continue;
        }

        let command = match catalog.parse_command(&input) {
            Ok(command) => command,
            Err(error) => {
                eprintln!("{error}");
                continue;
            }
        };

        let should_quit = matches!(command, SemanticCommand::Quit);
        let observation = runtime.execute(LOCAL_PLAYER_ID, &command, &catalog)?;
        print_observation(&observation, &catalog, cli.format)?;

        if should_quit {
            break;
        }
    }

    Ok(())
}

fn print_observation(
    observation: &JsonObservation,
    catalog: &Catalog,
    format: OutputFormat,
) -> Result<()> {
    match format {
        OutputFormat::Text => {
            print_text_observation(observation, catalog);
            Ok(())
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(observation)?);
            Ok(())
        }
    }
}

fn print_text_observation(observation: &JsonObservation, catalog: &Catalog) {
    println!();
    println!("{}", observation.title);
    println!("{}", observation.description);

    if !observation.exits.is_empty() {
        let exits = observation
            .exits
            .iter()
            .map(|exit| exit.direction.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        println!("{}: {exits}", catalog.text("label.exits"));
    }

    if !observation.entities.is_empty() {
        let entities = observation
            .entities
            .iter()
            .map(|entity| entity.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        println!("{}: {entities}", catalog.text("label.visible"));
    }

    for event in &observation.events {
        match event {
            ObservationEvent::Message { text } => println!("{text}"),
            ObservationEvent::Move { direction, .. } => {
                println!("{} {}", catalog.text("event.move"), direction.as_str());
            }
        }
    }
}

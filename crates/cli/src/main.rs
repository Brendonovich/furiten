//! `recall`: the command line, a client of the core the desktop app hosts. With no app running it starts one without a
//! window; with no app installed it hosts the core itself for the length of the command.

mod output;
mod review;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use recall_core::client::Client;
use recall_core::protocol::{Ack, CardQuery, CardSort, CardUpdate, DeckUpdate, NewCard, WebEnable};
use recall_core::server::{Hosted, Server};
use recall_core::{Error, Result};
use serde::Serialize;

#[derive(Parser)]
#[command(
    name = "recall",
    version,
    about = "Spaced-repetition flashcards. Cards are Markdown; a line with only --- splits the sides."
)]
struct Cli {
    /// Print the contract's JSON instead of text.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show the core's version, home and sync state.
    Status,
    /// List, add, rename, move or delete decks.
    Decks {
        #[command(subcommand)]
        command: Option<DecksCommand>,
    },
    /// List cards.
    #[command(args_conflicts_with_subcommands = true)]
    Cards {
        #[command(flatten)]
        filter: CardFilter,
        #[command(subcommand)]
        command: Option<CardsCommand>,
    },
    /// List the tags in use, the most recently used first.
    Tags {
        /// Those used in this deck (name or id) and its subdecks first.
        #[arg(long)]
        deck: Option<String>,
    },
    /// Add a card. The deck is created if no deck has that name.
    Add {
        /// Deck name or id.
        #[arg(long)]
        deck: String,
        /// A tag; repeat or separate with commas.
        #[arg(long = "tag", value_delimiter = ',')]
        tags: Vec<String>,
        /// Markdown, `---` on its own line between sides. `-` reads stdin.
        content: String,
    },
    /// Show a card.
    Show { id: String },
    /// Change a card's content, tags or deck.
    Edit {
        id: String,
        /// New Markdown; `-` reads stdin.
        #[arg(long)]
        content: Option<String>,
        /// Replace all tags (comma separated; "" clears them).
        #[arg(long, value_delimiter = ',')]
        tags: Option<Vec<String>>,
        /// Move to this deck (name or id).
        #[arg(long)]
        deck: Option<String>,
    },
    /// Delete a card.
    Rm { id: String },
    /// Review due cards interactively, or one step at a time with `next` and `answer`.
    #[command(args_conflicts_with_subcommands = true)]
    Review {
        /// Only this deck (name or id) and its subdecks.
        #[arg(long)]
        deck: Option<String>,
        #[command(subcommand)]
        command: Option<ReviewCommand>,
    },
    /// Counts, reviews today and 30-day retention.
    Stats {
        /// Only this deck (name or id) and its subdecks.
        #[arg(long)]
        deck: Option<String>,
    },
    /// Import decks from another app.
    Import {
        #[command(subcommand)]
        command: ImportCommand,
    },
    /// Sync now, or sign in to Recall Cloud or a self-hosted hub.
    Sync {
        #[command(subcommand)]
        command: Option<SyncCommand>,
    },
    /// Web access: let a phone or browser use this library over the network, through the desktop app.
    Web {
        #[command(subcommand)]
        command: Option<WebCommand>,
    },
    /// Hosting: this desktop as its library's hub, so phones pair with it and sync over Iroh.
    Host {
        #[command(subcommand)]
        command: Option<HostCommand>,
    },
    /// Show the desktop app's window.
    Open,
    /// Stop the core, and the desktop app hosting it.
    Shutdown,
}

#[derive(Subcommand)]
enum DecksCommand {
    /// List decks (the default).
    List,
    /// Create a deck.
    Add {
        name: String,
        /// Parent deck (name or id).
        #[arg(long)]
        parent: Option<String>,
    },
    /// Rename a deck.
    Rename { deck: String, name: String },
    /// Move a deck under another, or to the top level without --parent.
    Move {
        deck: String,
        #[arg(long)]
        parent: Option<String>,
    },
    /// Delete a deck, its subdecks and their cards.
    Rm { deck: String },
}

#[derive(Subcommand)]
enum CardsCommand {
    /// List cards (the default).
    List(CardFilter),
}

#[derive(Args, Default)]
struct CardFilter {
    /// Deck name or id (includes subdecks).
    #[arg(long)]
    deck: Option<String>,
    /// Text the content contains (case-insensitive).
    #[arg(long)]
    q: Option<String>,
    #[arg(long)]
    tag: Option<String>,
    /// Only cards that are due now.
    #[arg(long)]
    due: bool,
    #[arg(long, default_value_t = 50)]
    limit: u32,
    #[arg(long, default_value_t = 0)]
    offset: u32,
    /// Order: created (default), updated, name, due, reviewed, interval, retention or random.
    #[arg(long, value_parser = parse_sort)]
    sort: Option<CardSort>,
    /// Reverse the order (never-reviewed cards stay last).
    #[arg(long)]
    desc: bool,
    /// Seed for --sort random; the same seed gives the same order.
    #[arg(long)]
    seed: Option<u32>,
}

fn parse_sort(value: &str) -> std::result::Result<CardSort, String> {
    CardSort::parse(value).ok_or_else(|| {
        let all: Vec<&str> = CardSort::ALL.iter().map(|s| s.as_str()).collect();
        format!("expected one of {}", all.join(", "))
    })
}

#[derive(Subcommand)]
enum ReviewCommand {
    /// Print the next card to review.
    Next {
        #[arg(long)]
        deck: Option<String>,
    },
    /// Answer a card: whether you remembered it.
    Answer {
        id: String,
        answer: Recalled,
        /// How long the answer took.
        #[arg(long)]
        duration_ms: Option<i64>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Recalled {
    Remembered,
    Forgot,
}

#[derive(Subcommand)]
enum WebCommand {
    /// Where to open it and the token (the default).
    Status,
    /// Turn web access on; it stays on until turned off.
    On {
        /// Port to listen on (default 8787).
        #[arg(long)]
        port: Option<u16>,
        /// Address to listen on (default 0.0.0.0, every network; 127.0.0.1 for this machine only).
        #[arg(long)]
        bind: Option<String>,
    },
    /// Turn web access off.
    Off,
}

#[derive(Subcommand)]
enum HostCommand {
    /// Whether it's hosting, and the paired devices (the default).
    Status,
    /// Start hosting; it stays on until turned off.
    On,
    /// Stop hosting. Paired devices stay paired and reconnect when it's back on.
    Off,
    /// Make a one-time pairing ticket (valid 10 minutes) for a phone to scan, turning hosting on.
    Pair,
    /// Unpair a device by id, id prefix or name. It keeps what it already has.
    Rm { device: String },
}

#[derive(Subcommand)]
enum ImportCommand {
    /// Import a Mochi export (a .mochi file from Mochi's "Export deck" or "Export everything") as new decks, with
    /// its review history.
    Mochi {
        file: PathBuf,
        /// Put the imported decks under this deck (name or id).
        #[arg(long)]
        parent: Option<String>,
    },
}

#[derive(Subcommand)]
enum SyncCommand {
    /// Sign in to a server (without --token a new account is created; keep the token it prints to sign in
    /// elsewhere), or pair with a self-hosted hub using a ticket from `recall-hub pair`.
    Login {
        #[arg(long, required_unless_present = "hub", conflicts_with = "hub")]
        server: Option<String>,
        #[arg(long, requires = "server")]
        token: Option<String>,
        /// A hub's pairing ticket (rchub1...).
        #[arg(long)]
        hub: Option<String>,
        /// This device's name in the hub's device list (default: the host name).
        #[arg(long, requires = "hub")]
        name: Option<String>,
    },
    /// Sync now (the default).
    Now,
    /// Stop syncing. Everything stays on this device.
    Logout,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let json = cli.json;
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if json {
                println!("{}", serde_json::json!({ "error": error }));
            } else {
                eprintln!("recall: {}", error.message);
            }
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let home = recall_core::default_home();
    if let Command::Shutdown = cli.command
        && Client::connect(&home).is_err()
    {
        return emit(cli.json, &Ack { ok: true }, |_| "No core is running".to_string());
    }
    let (mut client, hosted) = connect(&home)?;
    let result = command(&mut client, cli);
    drop(client);
    match hosted {
        Some(hosted) => result.and(hosted.stop()),
        None => result,
    }
}

/// The core the desktop app hosts (starting the app without a window if needed), or, with no app installed, one
/// hosted here until the command ends.
fn connect(home: &Path) -> Result<(Client, Option<Hosted>)> {
    let missing = match Client::connect_or_spawn(home) {
        Ok(client) => return Ok((client, None)),
        Err(e) if e.code == "no_app" => e,
        Err(e) => return Err(e),
    };
    let server = match Server::bind(home) {
        Ok(server) => server,
        Err(e) if e.code == "already_running" => return Ok((Client::connect(home)?, None)),
        Err(e) => return Err(Error::new(&e.code, format!("{} (and {})", e.message, missing.message))),
    };
    let hosted = server.spawn(|| {});
    Ok((Client::connect(home)?, Some(hosted)))
}

fn command(c: &mut Client, cli: Cli) -> Result<()> {
    let json = cli.json;
    match cli.command {
        Command::Status => emit(json, &c.status()?, output::status),
        Command::Decks { command } => match command.unwrap_or(DecksCommand::List) {
            DecksCommand::List => emit(json, &c.decks()?, |d| output::decks(d)),
            DecksCommand::Add { name, parent } => {
                let parent = parent.map(|p| c.resolve_deck(&p, false)).transpose()?;
                let deck = c.create_deck(&name, parent.as_ref().map(|p| p.id.as_str()))?;
                emit(json, &deck, |d| format!("Created deck {} ({})", d.name, d.id))
            }
            DecksCommand::Rename { deck, name } => {
                let id = c.resolve_deck(&deck, false)?.id;
                let deck = c.update_deck(&DeckUpdate { id, name: Some(name), parent_id: None })?;
                emit(json, &deck, |d| format!("Renamed to {}", d.name))
            }
            DecksCommand::Move { deck, parent } => {
                let id = c.resolve_deck(&deck, false)?.id;
                let parent_id = parent.map(|p| c.resolve_deck(&p, false).map(|d| d.id)).transpose()?;
                let deck = c.update_deck(&DeckUpdate { id, name: None, parent_id: Some(parent_id) })?;
                emit(json, &deck, |d| format!("Moved {}", d.name))
            }
            DecksCommand::Rm { deck } => {
                let deck = c.resolve_deck(&deck, false)?;
                c.delete_deck(&deck.id)?;
                emit(json, &Ack { ok: true }, |_| format!("Deleted deck {}", deck.name))
            }
        },
        Command::Cards { filter, command } => {
            let filter = match command {
                Some(CardsCommand::List(filter)) => filter,
                None => filter,
            };
            let deck_id = filter.deck.map(|d| c.resolve_deck(&d, false).map(|d| d.id)).transpose()?;
            let query = CardQuery {
                deck_id,
                q: filter.q,
                tag: filter.tag,
                due: filter.due.then_some(true),
                limit: Some(filter.limit),
                offset: Some(filter.offset),
                sort: filter.sort,
                desc: filter.desc.then_some(true),
                seed: filter.seed,
            };
            emit(json, &c.cards(&query)?, |c| output::cards(c))
        }
        Command::Tags { deck } => {
            let deck_id = deck.map(|d| c.resolve_deck(&d, false).map(|d| d.id)).transpose()?;
            emit(json, &c.tags(deck_id.as_deref())?, |t| output::tags(t))
        }
        Command::Add { deck, tags, content } => {
            let deck = c.resolve_deck(&deck, true)?;
            let card = c.create_card(&NewCard { deck_id: deck.id, content: read_content(content)?, tags })?;
            emit(json, &card, |card| format!("Added {} to {}", card.card.id, deck.name))
        }
        Command::Show { id } => {
            let card = c.card(&id)?;
            let deck = c.decks()?.into_iter().find(|d| d.deck.id == card.card.deck_id).map(|d| d.deck.name);
            emit(json, &card, |card| output::card(card, deck.as_deref()))
        }
        Command::Edit { id, content, tags, deck } => {
            let deck_id = deck.map(|d| c.resolve_deck(&d, false).map(|d| d.id)).transpose()?;
            let content = content.map(read_content).transpose()?;
            let card = c.update_card(&CardUpdate { id, content, tags, deck_id })?;
            emit(json, &card, |card| format!("Updated {}", card.card.id))
        }
        Command::Rm { id } => {
            c.delete_card(&id)?;
            emit(json, &Ack { ok: true }, |_| format!("Deleted {id}"))
        }
        Command::Review { deck, command } => match command {
            None => {
                if json {
                    return Err(Error::invalid(
                        "interactive review has no JSON; use `review next` and `review answer`",
                    ));
                }
                let deck_id = deck.map(|d| c.resolve_deck(&d, false).map(|d| d.id)).transpose()?;
                review::interactive(c, deck_id.as_deref())
            }
            Some(ReviewCommand::Next { deck }) => {
                let deck_id = deck.map(|d| c.resolve_deck(&d, false).map(|d| d.id)).transpose()?;
                emit(json, &c.next_review(deck_id.as_deref())?, output::next)
            }
            Some(ReviewCommand::Answer { id, answer, duration_ms }) => {
                let remembered = matches!(answer, Recalled::Remembered);
                let card = c.answer(&id, remembered, duration_ms)?;
                emit(json, &card, |card| output::answered(remembered, card))
            }
        },
        Command::Stats { deck } => {
            let deck_id = deck.map(|d| c.resolve_deck(&d, false).map(|d| d.id)).transpose()?;
            emit(json, &c.stats(deck_id.as_deref())?, output::stats)
        }
        Command::Import { command: ImportCommand::Mochi { file, parent } } => {
            let parent = parent.map(|p| c.resolve_deck(&p, false)).transpose()?;
            let imported = c.import_mochi(&file, parent.as_ref().map(|p| p.id.as_str()))?;
            emit(json, &imported, output::imported)
        }
        Command::Sync { command } => match command.unwrap_or(SyncCommand::Now) {
            SyncCommand::Login { hub: Some(ticket), name, .. } => {
                let login = c.login_hub(&ticket, name.as_deref())?;
                emit(json, &login, |l| format!("Paired with the hub (library {})", l.library_id))
            }
            SyncCommand::Login { server, token, .. } => {
                let server = server.unwrap_or_default();
                let created = token.is_none();
                let login = c.login(&server, token.as_deref())?;
                emit(json, &login, |l| {
                    let mut text = format!("Signed in to {server} as {}", l.user_id);
                    if created {
                        text.push_str(&format!("\nNew account. To sign in on another device use --token {}", l.token));
                    }
                    text
                })
            }
            SyncCommand::Now => emit(json, &c.sync_now()?, |r| format!("Pushed {}, pulled {}", r.pushed, r.pulled)),
            SyncCommand::Logout => {
                c.logout()?;
                emit(json, &Ack { ok: true }, |_| "Signed out. Your decks stay here.".to_string())
            }
        },
        Command::Host { command } => match command.unwrap_or(HostCommand::Status) {
            HostCommand::Status => emit(json, &c.host_status()?, output::host),
            HostCommand::On => emit(json, &c.host_enable()?, output::host),
            HostCommand::Off => emit(json, &c.host_disable()?, output::host),
            HostCommand::Pair => emit(json, &c.host_pair()?, output::pair),
            HostCommand::Rm { device } => {
                emit(json, &c.host_remove(&device)?, |d| format!("Removed {} ({})", d.name, d.id))
            }
        },
        Command::Web { command } => match command.unwrap_or(WebCommand::Status) {
            WebCommand::Status => emit(json, &c.web_status()?, output::web),
            WebCommand::On { port, bind } => emit(json, &c.web_enable(&WebEnable { port, bind })?, output::web),
            WebCommand::Off => emit(json, &c.web_disable()?, output::web),
        },
        Command::Open => {
            c.open_app()?;
            emit(json, &Ack { ok: true }, |_| String::new())
        }
        Command::Shutdown => {
            c.shutdown()?;
            emit(json, &Ack { ok: true }, |_| "Core stopped".to_string())
        }
    }
}

/// Prints `value` as JSON, or as the text `human` makes from it.
fn emit<T: Serialize>(json: bool, value: &T, human: impl FnOnce(&T) -> String) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(value).map_err(Error::internal)?);
    } else {
        let text = human(value);
        if !text.is_empty() {
            println!("{text}");
        }
    }
    Ok(())
}

fn read_content(content: String) -> Result<String> {
    if content != "-" {
        return Ok(content);
    }
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    Ok(buf)
}

//! `recall`: the command line. `recall core` runs the core; every other command is a client of it.

mod output;
mod review;

use std::io::Read;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use recall_core::client::Client;
use recall_core::protocol::{Ack, CardQuery, CardUpdate, DeckUpdate, NewCard};
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
    /// Run the core in the foreground (other commands start it when needed).
    Core,
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
    /// Sync now, or sign in to a sync server.
    Sync {
        #[command(subcommand)]
        command: Option<SyncCommand>,
    },
    /// Stop the core.
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
}

#[derive(Subcommand)]
enum ReviewCommand {
    /// Print the next card to review.
    Next {
        #[arg(long)]
        deck: Option<String>,
    },
    /// Grade a card: 1 again, 2 hard, 3 good, 4 easy.
    Answer {
        id: String,
        #[arg(value_parser = clap::value_parser!(u8).range(1..=4))]
        rating: u8,
        /// How long the answer took.
        #[arg(long)]
        duration_ms: Option<i64>,
    },
}

#[derive(Subcommand)]
enum SyncCommand {
    /// Sign in. Without --token a new account is created; keep the token it prints to sign in elsewhere.
    Login {
        #[arg(long)]
        server: String,
        #[arg(long)]
        token: Option<String>,
    },
    /// Sync now (the default).
    Now,
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
    if let Command::Core = cli.command {
        let server = recall_core::server::Server::bind(&home)?;
        let socket = recall_core::server::socket_path(&home);
        eprintln!("recall core {} listening on {}", recall_core::VERSION, socket.display());
        return server.serve();
    }
    let mut client = Client::connect_or_spawn(&home)?;
    let c = &mut client;
    let json = cli.json;
    match cli.command {
        Command::Core => unreachable!(),
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
            };
            emit(json, &c.cards(&query)?, |c| output::cards(c))
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
            Some(ReviewCommand::Answer { id, rating, duration_ms }) => {
                let card = c.answer(&id, rating, duration_ms)?;
                emit(json, &card, |card| output::answered(rating, card))
            }
        },
        Command::Stats { deck } => {
            let deck_id = deck.map(|d| c.resolve_deck(&d, false).map(|d| d.id)).transpose()?;
            emit(json, &c.stats(deck_id.as_deref())?, output::stats)
        }
        Command::Sync { command } => match command.unwrap_or(SyncCommand::Now) {
            SyncCommand::Login { server, token } => {
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
        },
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

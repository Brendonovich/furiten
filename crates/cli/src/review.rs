//! Interactive review in a terminal: Enter (or Space) reveals, 1-4 grades, q quits.

use std::io::{IsTerminal, Read, Write};
use std::process::{Command, Stdio};
use std::time::Instant;

use recall_core::client::Client;
use recall_core::{Error, Result};

use crate::output;

pub fn interactive(client: &mut Client, deck_id: Option<&str>) -> Result<()> {
    if !std::io::stdin().is_terminal() {
        return Err(Error::invalid(
            "interactive review needs a terminal; use `recall review next` and `review answer`",
        ));
    }
    let _raw = RawMode::enable()?;
    let mut reviewed = 0;
    loop {
        let next = client.next_review(deck_id)?;
        let Some(card) = next.card else {
            println!("{}", if reviewed == 0 { "Nothing to review right now." } else { "All done for now." });
            break;
        };
        let started = Instant::now();
        println!("\n\x1b[2m{} due, {} new\x1b[0m\n", next.remaining.due, next.remaining.new);
        println!("{}", card.front());
        if card.sides.len() > 1 {
            prompt("[Enter] reveal  [q] quit");
            if !wait_for(|k| matches!(k, b'\n' | b'\r' | b' '))? {
                break;
            }
            for side in &card.sides[1..] {
                println!("\x1b[2m---\x1b[0m\n{side}");
            }
        }
        prompt("[1] again  [2] hard  [3] good  [4] easy  [q] quit");
        let mut rating = 0;
        if !wait_for(|k| {
            rating = if (b'1'..=b'4').contains(&k) { k - b'0' } else { 0 };
            rating != 0
        })? {
            break;
        }
        let answered = client.answer(&card.card.id, rating, Some(started.elapsed().as_millis() as i64))?;
        println!("\x1b[2m{}\x1b[0m", output::answered(rating, &answered));
        reviewed += 1;
    }
    if reviewed > 0 {
        println!("Reviewed {reviewed} card{}.", if reviewed == 1 { "" } else { "s" });
    }
    Ok(())
}

fn prompt(text: &str) {
    println!("\x1b[2m{text}\x1b[0m");
    let _ = std::io::stdout().flush();
}

/// Reads keys until `accept` takes one (true) or the user quits with q, Esc, Ctrl-C or Ctrl-D (false).
fn wait_for(mut accept: impl FnMut(u8) -> bool) -> Result<bool> {
    let mut stdin = std::io::stdin();
    let mut key = [0u8; 1];
    loop {
        if stdin.read(&mut key)? == 0 {
            return Ok(false);
        }
        match key[0] {
            b'q' | b'Q' | 0x1b | 3 | 4 => return Ok(false),
            k if accept(k) => return Ok(true),
            _ => {}
        }
    }
}

/// Single-key input via `stty`, restored on drop. Signals are off so Ctrl-C arrives as a key and the terminal
/// is always restored.
struct RawMode {
    saved: String,
}

impl RawMode {
    fn enable() -> Result<RawMode> {
        let saved = stty(&["-g"])?;
        stty(&["-icanon", "-echo", "-isig", "min", "1"])?;
        Ok(RawMode { saved: saved.trim().to_string() })
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = stty(&[&self.saved]);
    }
}

fn stty(args: &[&str]) -> Result<String> {
    let out = Command::new("stty").args(args).stdin(Stdio::inherit()).stderr(Stdio::inherit()).output()?;
    if !out.status.success() {
        return Err(Error::internal("stty failed"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

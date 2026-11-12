//! Interactive review and cramming in a terminal: Enter (or Space) reveals, 1 (or n) forgot, 2 (or y) remembered, q
//! quits.

use std::io::{IsTerminal, Read, Write};
use std::process::{Command, Stdio};
use std::time::Instant;

use furiten_core::client::Client;
use furiten_core::cram::Cram;
use furiten_core::{CardView, Error, Result};

use crate::output;

pub fn interactive(client: &mut Client, deck_id: Option<&str>) -> Result<()> {
    if !std::io::stdin().is_terminal() {
        return Err(Error::invalid(
            "interactive review needs a terminal; use `furiten review next` and `review answer`",
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
        let counts = format!("{} due, {} new", next.remaining.due, next.remaining.new);
        let Some(remembered) = ask(&card, &counts)? else { break };
        let answered = client.answer(&card.card.id, remembered, Some(started.elapsed().as_millis() as i64))?;
        println!("\x1b[2m{}\x1b[0m", output::answered(remembered, &answered));
        reviewed += 1;
    }
    if reviewed > 0 {
        println!("Reviewed {reviewed} card{}.", if reviewed == 1 { "" } else { "s" });
    }
    Ok(())
}

/// Practice on `cards` in order, due or not: nothing is answered to the core, a forgotten card comes round again at
/// the end, and it's done when each has been remembered once.
pub fn cram(cards: Vec<CardView>) -> Result<()> {
    if !std::io::stdin().is_terminal() {
        return Err(Error::invalid("cramming needs a terminal"));
    }
    if cards.is_empty() {
        println!("No cards to review.");
        return Ok(());
    }
    let _raw = RawMode::enable()?;
    let mut cram = Cram::new(cards);
    while let Some(card) = cram.current() {
        let Some(remembered) = ask(card, &format!("{} left", cram.left()))? else { break };
        println!("\x1b[2m{}\x1b[0m", if remembered { "Remembered" } else { "Forgot: it comes round again" });
        cram.answer(remembered);
    }
    let n = cram.remembered();
    if cram.finished() {
        println!("All {n} remembered. Nothing was scheduled.");
    } else if n > 0 {
        println!("Remembered {n} card{}. Nothing was scheduled.", if n == 1 { "" } else { "s" });
    }
    Ok(())
}

/// Shows a card under `header`, its other sides on Enter, and asks whether it was remembered: `None` to quit.
fn ask(card: &CardView, header: &str) -> Result<Option<bool>> {
    println!("\n\x1b[2m{header}\x1b[0m\n");
    println!("{}", card.front());
    if card.sides.len() > 1 {
        prompt("[Enter] reveal  [q] quit");
        if !wait_for(|k| matches!(k, b'\n' | b'\r' | b' '))? {
            return Ok(None);
        }
        for side in &card.sides[1..] {
            println!("\x1b[2m---\x1b[0m\n{side}");
        }
    }
    prompt("[1] forgot  [2] remembered  [q] quit");
    let mut remembered = None;
    if !wait_for(|k| {
        remembered = match k {
            b'1' | b'n' | b'N' => Some(false),
            b'2' | b'y' | b'Y' => Some(true),
            _ => None,
        };
        remembered.is_some()
    })? {
        return Ok(None);
    }
    Ok(remembered)
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

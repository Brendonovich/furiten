use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use super::*;
use crate::client::{Client, Event, Events};

fn home() -> PathBuf {
    // Short: Unix socket paths are limited to about 100 bytes.
    let id = crate::model::ulid();
    std::env::temp_dir().join(format!("rc-{}", &id[16..]))
}

fn start(home: &Path) -> std::thread::JoinHandle<Result<()>> {
    let server = Server::bind(home).unwrap();
    std::thread::spawn(move || server.serve())
}

#[test]
fn clients_talk_to_the_core_and_hear_about_changes() {
    let home = home();
    let core = start(&home);
    let mode = std::fs::metadata(socket_path(&home)).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);

    let mut a = Client::connect(&home).unwrap();
    let mut b = Client::connect(&home).unwrap();
    let (tx, rx) = mpsc::channel();
    let events = a.events().unwrap();
    std::thread::spawn(move || {
        for event in events {
            tx.send(event).unwrap();
        }
    });

    let status = a.status().unwrap();
    assert_eq!((status.version.as_str(), status.signed_in, status.server), (VERSION, false, None));
    let deck = a.create_deck("Spanish", None).unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), Event::Changed);

    let cards = b
        .create_cards(&[
            NewCard { deck_id: deck.id.clone(), content: "hola\n---\nhello".into(), tags: vec!["Greeting".into()] },
            NewCard { deck_id: deck.id.clone(), content: "adiós\n---\ngoodbye".into(), tags: vec![] },
        ])
        .unwrap();
    assert_eq!(cards[0].card.tags, ["greeting"]);
    // `a` still gets replies even though events for b's write are queued on its connection.
    assert_eq!(a.decks().unwrap().len(), 2);
    let next = a.next_review(Some(&deck.id)).unwrap();
    assert_eq!(next.remaining, Remaining { due: 0, new: 2 });
    let answered = a.answer(&next.card.unwrap().card.id, 3, Some(1500)).unwrap();
    assert_eq!(answered.schedule.state, crate::State::Learning);
    assert_eq!(b.stats(Some(&deck.id)).unwrap().reviews_today, 1);
    assert_eq!(b.resolve_deck("spanish", false).unwrap().id, deck.id);
    assert_eq!(b.resolve_deck("French", true).unwrap().name, "French");

    let err = a.card("nope").unwrap_err();
    assert_eq!(err.code, "not_found");
    let err = a.call::<Value>("nope", json!({})).unwrap_err();
    assert_eq!(err.code, "unknown_method");
    let err = a.call::<Value>("cards.create", json!({ "content": 1 })).unwrap_err();
    assert_eq!(err.code, "invalid_params");
    assert_eq!(a.sync_now().unwrap_err().code, "not_signed_in");

    // The second instance refuses while the first runs.
    assert_eq!(Server::bind(&home).err().unwrap().code, "already_running");

    a.shutdown().unwrap();
    core.join().unwrap().unwrap();
    assert!(!socket_path(&home).exists());
    assert!(Client::connect(&home).is_err());

    // A restart (here over a stale socket file) keeps the data.
    std::fs::write(socket_path(&home), b"").unwrap();
    let core = start(&home);
    let mut c = Client::connect(&home).unwrap();
    assert_eq!(c.cards(&CardQuery { deck_id: Some(deck.id), ..Default::default() }).unwrap().len(), 2);
    c.shutdown().unwrap();
    core.join().unwrap().unwrap();
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_client_that_never_reads_does_not_block_the_core() {
    let home = home();
    let core = start(&home);
    let _idle = Events::connect(&home).unwrap();
    let mut c = Client::connect(&home).unwrap();
    let deck = c.create_deck("Busy", None).unwrap();
    for i in 0..2000 {
        c.update_deck(&DeckUpdate { id: deck.id.clone(), name: Some(format!("Busy {i}")), parent_id: None }).unwrap();
    }
    c.shutdown().unwrap();
    core.join().unwrap().unwrap();
    std::fs::remove_dir_all(home).unwrap();
}

use std::sync::Mutex;

use super::memory::MemoryServer;
use super::*;
use crate::Change;
use crate::protocol::{Answer, CardQuery, CardUpdate, NewCard, NewDeck};

const T0: i64 = 1_800_000_000_000;
const DAY: i64 = 86_400_000;

fn device(now: i64) -> Mutex<Store> {
    let mut store = Store::open_in_memory().unwrap();
    store.now = Some(now);
    Mutex::new(store)
}

fn sync(store: &Mutex<Store>, server: &dyn Transport) -> SyncResult {
    sync_once(store, server).unwrap().0
}

fn set_now(store: &Mutex<Store>, now: i64) {
    lock(store).now = Some(now);
}

fn everything(store: &Mutex<Store>) -> (Vec<crate::DeckSummary>, Vec<crate::CardView>) {
    let s = lock(store);
    (s.decks().unwrap(), s.cards(&CardQuery { limit: Some(1000), ..Default::default() }).unwrap())
}

#[test]
fn two_devices_converge_without_duplicating_the_sample_deck() {
    let server = MemoryServer::default();
    let (a, b) = (device(T0), device(T0));
    let deck = lock(&a).create_deck(NewDeck { name: "Spanish".into(), parent_id: None }).unwrap();
    let card = lock(&a)
        .create_card(NewCard { deck_id: deck.id.clone(), content: "hola\n---\nhello".into(), tags: vec![] })
        .unwrap();

    assert_eq!(sync(&a, &server), SyncResult { pushed: 8, pulled: 0 });
    let first = sync(&b, &server);
    assert_eq!(first.pushed, 6, "b pushes its own copy of the sample deck");
    assert_eq!(first.pulled, 2, "b only pulls what it doesn't have; the identical sample deck isn't echoed");
    assert_eq!(lock(&b).decks().unwrap().len(), 2);

    set_now(&b, T0 + 1000);
    lock(&b).answer(Answer { card_id: card.card.id.clone(), rating: 3, duration_ms: None }).unwrap();
    lock(&b)
        .update_card(CardUpdate { id: card.card.id.clone(), tags: Some(vec!["greeting".into()]), ..Default::default() })
        .unwrap();
    assert_eq!(sync(&b, &server).pushed, 2);
    assert_eq!(sync(&a, &server).pulled, 2);
    assert_eq!(everything(&a), everything(&b));
    assert_eq!(lock(&a).card(&card.card.id).unwrap().card.tags, ["greeting"]);

    assert_eq!(sync(&a, &server), SyncResult { pushed: 0, pulled: 0 });
    assert_eq!(sync(&b, &server), SyncResult { pushed: 0, pulled: 0 });
}

#[test]
fn the_later_edit_wins_on_both_devices_and_deletes_sync() {
    let server = MemoryServer::default();
    let (a, b) = (device(T0), device(T0));
    let deck = lock(&a).create_deck(NewDeck { name: "D".into(), parent_id: None }).unwrap();
    let card = lock(&a).create_card(NewCard { deck_id: deck.id.clone(), content: "v0".into(), tags: vec![] }).unwrap();
    sync(&a, &server);
    sync(&b, &server);

    // Offline: b edits later than a.
    set_now(&a, T0 + 10);
    set_now(&b, T0 + 20);
    let edit = |s: &Mutex<Store>, content: &str| {
        lock(s)
            .update_card(CardUpdate { id: card.card.id.clone(), content: Some(content.into()), ..Default::default() })
            .unwrap()
    };
    edit(&a, "from a");
    edit(&b, "from b");
    sync(&b, &server);
    sync(&a, &server);
    sync(&b, &server);
    for s in [&a, &b] {
        assert_eq!(lock(s).card(&card.card.id).unwrap().card.content, "from b");
    }

    set_now(&a, T0 + 30);
    lock(&a).delete_deck(&deck.id).unwrap();
    sync(&a, &server);
    sync(&b, &server);
    assert_eq!(lock(&b).card(&card.card.id).unwrap_err().code, "not_found");
    assert_eq!(everything(&a), everything(&b));
}

#[test]
fn offline_reviews_on_two_devices_merge_into_one_schedule() {
    let server = MemoryServer::default();
    let (a, b) = (device(T0), device(T0));
    let deck = lock(&a).create_deck(NewDeck { name: "D".into(), parent_id: None }).unwrap();
    let card = lock(&a).create_card(NewCard { deck_id: deck.id, content: "q".into(), tags: vec![] }).unwrap();
    let id = card.card.id;
    sync(&a, &server);
    sync(&b, &server);

    let review = |s: &Mutex<Store>, at: i64, rating: u8| {
        set_now(s, at);
        lock(s).answer(Answer { card_id: id.clone(), rating, duration_ms: None }).unwrap();
    };
    review(&a, T0 + DAY, 4);
    review(&b, T0 + 2 * DAY, 1);
    review(&a, T0 + 3 * DAY, 3);
    // b syncs first, so a receives a review older than its latest one.
    sync(&b, &server);
    sync(&a, &server);
    sync(&b, &server);

    let schedule = |s: &Mutex<Store>| lock(s).card(&id).unwrap().schedule;
    assert_eq!(schedule(&a), schedule(&b));
    assert_eq!(schedule(&a).reps, 3);
    assert_eq!(schedule(&a).lapses, 1);
}

/// A transport that makes a local edit while the request is in flight, then forwards to the server.
struct EditDuringRequest<'a> {
    store: &'a Mutex<Store>,
    server: &'a MemoryServer,
    card_id: String,
}

impl Transport for EditDuringRequest<'_> {
    fn sync(&self, request: &crate::protocol::SyncRequest) -> crate::Result<crate::protocol::SyncResponse> {
        lock(self.store)
            .update_card(CardUpdate {
                id: self.card_id.clone(),
                content: Some("mid-flight".into()),
                ..Default::default()
            })
            .unwrap();
        self.server.sync(request)
    }
}

#[test]
fn edits_made_during_a_sync_stay_in_the_outbox_and_resending_is_harmless() {
    let server = MemoryServer::default();
    let a = device(T0);
    let deck = lock(&a).create_deck(NewDeck { name: "D".into(), parent_id: None }).unwrap();
    let card = lock(&a).create_card(NewCard { deck_id: deck.id, content: "q".into(), tags: vec![] }).unwrap();
    let racy = EditDuringRequest { store: &a, server: &server, card_id: card.card.id.clone() };
    sync(&a, &racy);
    assert_eq!(lock(&a).pending().unwrap().changes.len(), 1, "the mid-flight edit is still unsent");

    // A lost response: the server stored the changes but the device never heard back.
    let outbox = lock(&a).pending().unwrap();
    server.sync(&crate::protocol::SyncRequest { cursor: None, changes: outbox.changes }).unwrap();
    let entities = server.len();
    assert_eq!(sync(&a, &server).pushed, 1);
    assert_eq!(server.len(), entities, "resending stored nothing new");

    let b = device(T0);
    sync(&b, &server);
    assert_eq!(lock(&b).card(&card.card.id).unwrap().card.content, "mid-flight");
    assert_eq!(server.requests(), 4);
}

#[test]
fn signing_in_to_another_account_resends_everything() {
    let server = MemoryServer::default();
    let a = device(T0);
    sync(&a, &server);
    assert!(lock(&a).pending().unwrap().changes.is_empty());
    let config = |user: &str| SyncConfig {
        server: "https://x".into(),
        token: format!("rc_{user}_s"),
        user_id: user.into(),
        hub: None,
    };
    lock(&a).set_sync_config(&config("u1")).unwrap();
    assert_eq!(lock(&a).pending().unwrap().changes.len(), 6);
    assert_eq!(lock(&a).cursor().unwrap(), None);
    sync(&a, &server);
    lock(&a).set_sync_config(&config("u1")).unwrap();
    assert!(lock(&a).pending().unwrap().changes.is_empty(), "same account, nothing to resend");
    assert_eq!(token_user_id("rc_01ABC_secret").unwrap(), "01ABC");
    assert!(token_user_id("nope").is_err());
}

#[test]
fn a_hub_answers_a_losing_write_with_the_stored_version_and_never_echoes_what_it_was_sent() {
    let mut hub = Store::open_hub_in_memory().unwrap();
    assert!(hub.decks().unwrap().is_empty(), "a hub has no sample deck");
    let deck = crate::Deck {
        id: crate::model::ulid(),
        name: "D".into(),
        parent_id: None,
        created_at: T0,
        updated_at: T0,
        deleted: false,
    };
    let card = |at: i64, content: &str| crate::Card {
        id: "01CARD".into(),
        deck_id: deck.id.clone(),
        content: content.into(),
        tags: vec!["#B".into(), "a".into()],
        created_at: T0,
        updated_at: at,
        deleted: false,
    };
    let review = crate::Review {
        id: "01REVIEW".into(),
        card_id: "01CARD".into(),
        rating: 3,
        reviewed_at: T0 + 5,
        duration_ms: None,
    };
    let request = |cursor, changes| crate::protocol::SyncRequest { cursor, changes };

    let (first, accepted) = hub
        .serve_sync(&request(
            None,
            vec![Change::Deck(deck.clone()), Change::Card(card(T0 + 20, "new")), Change::Review(review.clone())],
        ))
        .unwrap();
    assert_eq!((first.changes.len(), accepted, first.cursor), (0, 3, hub.seq().unwrap()));
    assert_eq!(hub.card("01CARD").unwrap().card.tags, ["a", "b"], "tags are normalised on the way in");
    assert_eq!(hub.card("01CARD").unwrap().schedule.reps, 1, "the hub derives schedules too");

    // An older edit from a device that's already up to date loses, and the device gets the winner back.
    let (lost, accepted) =
        hub.serve_sync(&request(Some(first.cursor), vec![Change::Card(card(T0 + 10, "old"))])).unwrap();
    assert_eq!((accepted, lost.cursor), (0, first.cursor));
    assert!(matches!(&lost.changes[..], [Change::Card(c)] if c.content == "new"));

    // Resending what's stored (the same card, deck and review) is harmless and echoes nothing.
    let (again, accepted) = hub
        .serve_sync(&request(
            None,
            vec![Change::Deck(deck.clone()), Change::Card(hub.card("01CARD").unwrap().card), Change::Review(review)],
        ))
        .unwrap();
    assert_eq!((again.changes.len(), accepted, again.cursor), (0, 0, first.cursor));

    // A new device gets everything, in seq order.
    let (all, _) = hub.serve_sync(&request(None, vec![])).unwrap();
    assert_eq!(all.changes.iter().map(|c| c.key().0).collect::<Vec<_>>(), ["deck", "card", "review"]);

    assert_eq!(hub.apply(&[]).unwrap_err().code, "wrong_store");
    assert_eq!(Store::open_in_memory().unwrap().serve_sync(&request(None, vec![])).unwrap_err().code, "wrong_store");
    let dir = std::env::temp_dir().join(format!("recall-hubstore-{}", crate::model::ulid()));
    drop(Store::open_hub(&dir.join("hub.db")).unwrap());
    assert_eq!(Store::open(&dir.join("hub.db")).err().unwrap().code, "wrong_store");
    drop(Store::open(&dir.join("device.db")).unwrap());
    assert_eq!(Store::open_hub(&dir.join("device.db")).err().unwrap().code, "wrong_store");
    std::fs::remove_dir_all(dir).unwrap();
}

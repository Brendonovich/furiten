use super::*;
use crate::model::{Change, Review};
use crate::protocol::{Answer, Remaining};

const T0: i64 = 1_800_000_000_000;
const MIN: i64 = 60_000;
const DAY: i64 = 86_400_000;

fn store() -> Store {
    let mut store = Store::open_in_memory().unwrap();
    store.now = Some(T0);
    store
}

fn deck(store: &mut Store, name: &str, parent: Option<&str>) -> Deck {
    store.create_deck(NewDeck { name: name.into(), parent_id: parent.map(Into::into) }).unwrap()
}

fn card(store: &mut Store, deck_id: &str, content: &str) -> CardView {
    store.create_card(NewCard { deck_id: deck_id.into(), content: content.into(), tags: vec![] }).unwrap()
}

fn answer(store: &mut Store, card_id: &str, remembered: bool) -> CardView {
    store.answer(Answer { card_id: card_id.into(), remembered, duration_ms: Some(1000) }).unwrap()
}

#[test]
fn a_new_store_has_the_sample_deck_once() {
    let dir = std::env::temp_dir().join(format!("recall-store-{}", ulid()));
    let path = dir.join("recall.db");
    let store = Store::open(&path).unwrap();
    let decks = store.decks().unwrap();
    assert_eq!(decks.len(), 1);
    assert_eq!(decks[0].deck.name, "Recall basics");
    assert_eq!((decks[0].cards, decks[0].new), (5, 5));
    let next = store.next_review(None).unwrap();
    assert!(next.card.unwrap().front().contains("---"));
    let device = store.device_id().unwrap();
    assert_eq!(device.len(), 26);
    drop(store);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.decks().unwrap().len(), 1);
    assert_eq!(store.device_id().unwrap(), device);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn deleting_a_deck_soft_deletes_its_subdecks_and_cards() {
    let mut s = store();
    let a = deck(&mut s, "A", None);
    let b = deck(&mut s, "B", Some(&a.id));
    let ca = card(&mut s, &a.id, "a");
    let cb = card(&mut s, &b.id, "b");
    let summary = s.decks().unwrap().into_iter().find(|d| d.deck.id == a.id).unwrap();
    assert_eq!(summary.cards, 2, "counts include subdecks");

    s.delete_deck(&a.id).unwrap();
    assert!(s.decks().unwrap().iter().all(|d| d.deck.id != a.id && d.deck.id != b.id));
    assert_eq!(s.card(&ca.card.id).unwrap_err().code, "not_found");
    assert_eq!(s.card(&cb.card.id).unwrap_err().code, "not_found");
    let deleted = s
        .pending()
        .unwrap()
        .changes
        .into_iter()
        .filter(|c| match c {
            Change::Deck(d) => d.deleted,
            Change::Card(c) => c.deleted,
            Change::Review(_) => false,
        })
        .count();
    assert_eq!(deleted, 4);
}

#[test]
fn decks_cannot_move_inside_themselves() {
    let mut s = store();
    let a = deck(&mut s, "A", None);
    let b = deck(&mut s, "B", Some(&a.id));
    let err = s.update_deck(DeckUpdate { id: a.id.clone(), name: None, parent_id: Some(Some(b.id.clone())) });
    assert_eq!(err.unwrap_err().code, "invalid_params");
    let moved = s.update_deck(DeckUpdate { id: b.id, name: Some(" B2 ".into()), parent_id: Some(None) }).unwrap();
    assert_eq!((moved.name.as_str(), moved.parent_id), ("B2", None));
    assert!(s.create_deck(NewDeck { name: "  ".into(), parent_id: None }).is_err());
}

#[test]
fn cards_list_filters() {
    let mut s = store();
    let a = deck(&mut s, "A", None);
    let b = deck(&mut s, "B", Some(&a.id));
    let one = s
        .create_card(NewCard {
            deck_id: a.id.clone(),
            content: "Hola\n---\nHello".into(),
            tags: vec!["#Spanish".into()],
        })
        .unwrap();
    assert_eq!(one.card.tags, ["spanish"]);
    assert_eq!(one.sides, ["Hola", "Hello"]);
    let two = card(&mut s, &b.id, "Adiós\n---\nGoodbye");
    answer(&mut s, &two.card.id, true);

    let (one_id, two_id) = (one.card.id.as_str(), two.card.id.as_str());
    let list = |s: &Store, q: CardQuery| s.cards(&q).unwrap().into_iter().map(|c| c.card.id).collect::<Vec<_>>();
    let in_a = CardQuery { deck_id: Some(a.id.clone()), ..Default::default() };
    assert_eq!(list(&s, in_a.clone()), [one_id, two_id]);
    assert_eq!(list(&s, CardQuery { deck_id: Some(b.id.clone()), ..Default::default() }), [two_id]);
    assert_eq!(list(&s, CardQuery { q: Some("HOLA".into()), ..in_a.clone() }), [one_id]);
    assert_eq!(list(&s, CardQuery { tag: Some("#SPANISH".into()), ..in_a.clone() }), [one_id]);
    assert_eq!(list(&s, CardQuery { due: Some(true), ..in_a.clone() }), [one_id]);
    assert_eq!(list(&s, CardQuery { limit: Some(1), offset: Some(1), ..in_a.clone() }), [two_id]);
    assert_eq!(s.cards(&CardQuery::default()).unwrap().len(), 7);
    assert_eq!(
        s.cards(&CardQuery { deck_id: Some("nope".into()), ..Default::default() }).unwrap_err().code,
        "not_found"
    );
}

#[test]
fn tags_list_the_most_recently_used_first() {
    let mut store = store();
    let d = deck(&mut store, "D", None);
    let tagged = |store: &mut Store, tags: &[&str]| {
        let tags = tags.iter().map(|t| t.to_string()).collect();
        store.create_card(NewCard { deck_id: d.id.clone(), content: "x".into(), tags }).unwrap()
    };
    let old = tagged(&mut store, &["vocab", "food"]);
    store.now = Some(T0 + MIN);
    tagged(&mut store, &["Verbs"]);
    store.now = Some(T0 + 2 * MIN);
    let gone = tagged(&mut store, &["gone", "vocab"]);
    store.delete_card(&gone.card.id).unwrap();
    let summary = |store: &Store| -> Vec<(String, u64)> {
        store.tags(None).unwrap().into_iter().map(|t| (t.tag, t.cards)).collect()
    };
    let expect =
        |list: &[(&str, u64)]| -> Vec<(String, u64)> { list.iter().map(|(t, n)| (t.to_string(), *n)).collect() };
    assert_eq!(
        summary(&store),
        expect(&[("verbs", 1), ("food", 1), ("vocab", 1), ("recall", 5)]),
        "deleted cards' tags don't count"
    );

    store.now = Some(T0 + 3 * MIN);
    store.update_card(CardUpdate { id: old.card.id, content: Some("y".into()), tags: None, deck_id: None }).unwrap();
    assert_eq!(
        summary(&store),
        expect(&[("food", 1), ("vocab", 1), ("verbs", 1), ("recall", 5)]),
        "an edit uses the card's tags again"
    );
    assert_eq!(store.tags(None).unwrap()[0].last_used, T0 + 3 * MIN);
}

#[test]
fn tags_used_in_a_deck_come_first_for_it() {
    let mut store = store();
    let spanish = deck(&mut store, "Spanish", None);
    let verbs = deck(&mut store, "Verbs", Some(&spanish.id));
    let rust = deck(&mut store, "Rust", None);
    let tagged = |store: &mut Store, deck_id: &str, tag: &str| {
        store.create_card(NewCard { deck_id: deck_id.into(), content: "x".into(), tags: vec![tag.into()] }).unwrap();
    };
    tagged(&mut store, &spanish.id, "vocab");
    store.now = Some(T0 + MIN);
    tagged(&mut store, &verbs.id, "irregular");
    store.now = Some(T0 + 2 * MIN);
    tagged(&mut store, &rust.id, "borrowing");
    let names = |deck: Option<&str>| -> Vec<String> {
        store.tags(deck).unwrap().into_iter().map(|t| t.tag).filter(|t| t != "recall").collect()
    };
    assert_eq!(names(None), ["borrowing", "irregular", "vocab"]);
    assert_eq!(names(Some(&spanish.id)), ["irregular", "vocab", "borrowing"], "subdecks count, the rest follow");
    assert_eq!(names(Some(&verbs.id)), ["irregular", "borrowing", "vocab"]);
    assert_eq!(store.tags(Some("nope")).unwrap_err().code, "not_found");
}

#[test]
fn cards_list_sorts_then_pages() {
    let mut s = store();
    let a = deck(&mut s, "A", None);
    let ten = card(&mut s, &a.id, "Card 10");
    s.now = Some(T0 + 1);
    let nine = card(&mut s, &a.id, "card 9");
    s.now = Some(T0 + 2);
    let apple = card(&mut s, &a.id, "**Apple**");
    answer(&mut s, &nine.card.id, true);

    let list = |s: &Store, q: CardQuery| s.cards(&q).unwrap().into_iter().map(|c| c.card.id).collect::<Vec<_>>();
    let in_a = CardQuery { deck_id: Some(a.id.clone()), ..Default::default() };
    let (ten, nine, apple) = (ten.card.id.as_str(), nine.card.id.as_str(), apple.card.id.as_str());
    assert_eq!(list(&s, CardQuery { desc: Some(true), ..in_a.clone() }), [apple, nine, ten]);
    let by_name = CardQuery { sort: Some(CardSort::Name), ..in_a.clone() };
    assert_eq!(list(&s, by_name.clone()), [apple, nine, ten]);
    assert_eq!(list(&s, CardQuery { limit: Some(1), offset: Some(1), ..by_name }), [nine]);
    assert_eq!(
        list(&s, CardQuery { sort: Some(CardSort::Reviewed), desc: Some(true), ..in_a.clone() }),
        [nine, ten, apple]
    );
    let shuffled = CardQuery { sort: Some(CardSort::Random), seed: Some(3), ..in_a };
    assert_eq!(list(&s, shuffled.clone()), list(&s, shuffled));
}

#[test]
fn edits_always_move_updated_at_forward() {
    let mut s = store();
    let a = deck(&mut s, "A", None);
    let c = card(&mut s, &a.id, "q");
    s.now = Some(T0 - DAY);
    let edited = s
        .update_card(CardUpdate {
            id: c.card.id.clone(),
            content: Some("q2".into()),
            tags: Some(vec!["X".into()]),
            deck_id: None,
        })
        .unwrap();
    assert_eq!(edited.card.updated_at, T0 + 1);
    assert_eq!((edited.card.content.as_str(), edited.card.tags.clone()), ("q2", vec!["x".to_string()]));
    s.delete_card(&c.card.id).unwrap();
    assert!(s.cards(&CardQuery { deck_id: Some(a.id), ..Default::default() }).unwrap().is_empty());
}

#[test]
fn the_queue_puts_due_cards_first_then_new_cards_capped_per_deck_per_day() {
    let mut s = store();
    let a = deck(&mut s, "A", None);
    let cards: Vec<CardView> = (0..25)
        .map(|i| {
            s.now = Some(T0 - 100 + i);
            card(&mut s, &a.id, &format!("card {i}"))
        })
        .collect();
    s.now = Some(T0);
    let next = |s: &Store| s.next_review(Some(&a.id)).unwrap();
    assert_eq!(next(&s).remaining, Remaining { due: 0, new: 20 });
    assert_eq!(next(&s).card.unwrap().card.id, cards[0].card.id, "new cards come oldest first");

    // Answer the first two; the second goes back into learning sooner, so it's due first.
    answer(&mut s, &cards[0].card.id, true);
    answer(&mut s, &cards[1].card.id, false);
    assert_eq!(next(&s).remaining, Remaining { due: 0, new: 18 });
    s.now = Some(T0 + 2 * MIN);
    let n = next(&s);
    assert_eq!(n.remaining, Remaining { due: 1, new: 18 });
    assert_eq!(n.card.unwrap().card.id, cards[1].card.id);

    for c in &cards[2..20] {
        answer(&mut s, &c.card.id, true);
    }
    assert_eq!(next(&s).remaining.new, 0, "20 new cards introduced today");
    // The sample deck has its own allowance.
    assert_eq!(s.next_review(None).unwrap().remaining.new, 5);

    s.now = Some(T0 + DAY + MIN);
    assert_eq!(next(&s).remaining.new, 5, "a new day, the rest of the new cards");
}

#[test]
fn schedules_are_the_fold_of_reviews_even_out_of_order() {
    let mut s = store();
    let a = deck(&mut s, "A", None);
    let c = card(&mut s, &a.id, "q\n---\na");
    let id = c.card.id.clone();
    let review = |n: i64, rating: u8| Review {
        id: crate::model::ulid_from(T0 + n, n as u128),
        card_id: id.clone(),
        rating,
        reviewed_at: T0 + n * DAY,
        duration_ms: None,
    };
    // Arrive newest first, as sync might deliver them.
    s.apply(&[Change::Review(review(3, 3)), Change::Review(review(1, 3))]).unwrap();
    s.apply(&[Change::Review(review(2, 1)), Change::Review(review(1, 3))]).unwrap();

    let fsrs = recall_srs::Scheduler::default();
    use recall_srs::Rating::*;
    let expected = fsrs.fold(T0, [(T0 + DAY, Good), (T0 + 2 * DAY, Again), (T0 + 3 * DAY, Good)]);
    assert_eq!(s.card(&id).unwrap().schedule, Schedule::from(expected));
}

#[test]
fn answers_validate_and_schedule() {
    let mut s = store();
    let a = deck(&mut s, "A", None);
    let c = card(&mut s, &a.id, "q");
    let bad = s.answer(Answer { card_id: c.card.id.clone(), remembered: true, duration_ms: Some(-1) });
    assert_eq!(bad.unwrap_err().code, "invalid_params");
    assert_eq!(
        s.answer(Answer { card_id: "nope".into(), remembered: true, duration_ms: None }).unwrap_err().code,
        "not_found"
    );
    let answered = answer(&mut s, &c.card.id, true);
    assert_eq!(answered.schedule.state, State::Learning);
    assert_eq!(answered.schedule.due, T0 + 10 * MIN);
    assert_eq!(answered.schedule.last_review, Some(T0));
}

#[test]
fn remembered_and_forgot_schedule_as_good_and_again() {
    let mut s = store();
    let a = deck(&mut s, "A", None);
    let c = card(&mut s, &a.id, "q\n---\na");
    let fsrs = recall_srs::Scheduler::default();
    use recall_srs::Rating::*;
    let answers = [(T0, true), (T0 + 10 * MIN, true), (T0 + 5 * DAY, false), (T0 + 5 * DAY + 10 * MIN, true)];
    for (at, remembered) in answers {
        s.now = Some(at);
        answer(&mut s, &c.card.id, remembered);
    }
    let expected = fsrs.fold(T0, answers.map(|(at, remembered)| (at, if remembered { Good } else { Again })));
    let schedule = s.card(&c.card.id).unwrap().schedule;
    assert_eq!(schedule, Schedule::from(expected));
    assert_eq!((schedule.state, schedule.reps, schedule.lapses), (State::Review, 4, 1));
}

#[test]
fn stats_count_retention_only_for_cards_in_review() {
    let mut s = store();
    let a = deck(&mut s, "A", None);
    let c1 = card(&mut s, &a.id, "1");
    let c2 = card(&mut s, &a.id, "2");
    let empty = s.stats(Some(&a.id)).unwrap();
    assert_eq!((empty.cards, empty.due, empty.new, empty.reviews_today, empty.retention_30d), (2, 0, 2, 0, None));

    // Remembered twice graduates both to review; answers while learning don't count towards retention.
    for at in [T0, T0 + 10 * MIN] {
        s.now = Some(at);
        answer(&mut s, &c1.card.id, true);
        answer(&mut s, &c2.card.id, true);
    }
    s.now = Some(T0 + 20 * DAY);
    answer(&mut s, &c1.card.id, true);
    answer(&mut s, &c2.card.id, false);
    let stats = s.stats(Some(&a.id)).unwrap();
    assert_eq!(stats.retention_30d, Some(0.5));
    assert_eq!(stats.reviews_today, 2);
    assert_eq!(stats.new, 0);
    let all = s.stats(None).unwrap();
    assert_eq!((all.cards, all.new), (7, 5));
}

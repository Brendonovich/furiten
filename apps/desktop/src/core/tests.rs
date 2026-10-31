use super::*;

#[test]
fn types_read_the_contracts_json() {
    let deck: DeckSummary = serde_json::from_str(
        r#"{"id":"01J0000000000000000000DECK","name":"Spanish","parent_id":null,"created_at":1,"updated_at":2,
            "deleted":false,"cards":12,"due":3,"new":4}"#,
    )
    .unwrap();
    assert_eq!((deck.deck.name.as_str(), deck.due, deck.new), ("Spanish", 3, 4));

    let next: NextReview = serde_json::from_str(
        r#"{"card":{"id":"C","deck_id":"D","content":"hola\n---\nhello","tags":["vocab"],"created_at":1,
            "updated_at":1,"deleted":false,"sides":["hola","hello"],
            "schedule":{"state":"new","due":1,"stability":0,"difficulty":0,"reps":0,"lapses":0,"last_review":null}},
            "remaining":{"due":0,"new":1}}"#,
    )
    .unwrap();
    let card = next.card.unwrap();
    assert_eq!(card.sides, ["hola", "hello"]);
    assert_eq!(card.schedule.state, State::New);
}

#[test]
fn sides_split_on_rules() {
    assert_eq!(sides("a\n---\nb\n---\n\nc\n"), ["a", "b", "c"]);
    assert_eq!(sides("just a note"), ["just a note"]);
}

#[test]
fn fixtures_advance_the_queue() {
    let mut core = Core::fixtures(Fixtures::FirstRun);
    let first = core.next_review(None).unwrap();
    assert_eq!(first.remaining, Remaining { due: 0, new: 5 });
    let card = first.card.unwrap();
    core.answer(Answer { card_id: card.card.id.clone(), rating: 3, duration_ms: None }).unwrap();
    let next = core.next_review(None).unwrap();
    assert_eq!(next.remaining.new, 4);
    assert_ne!(next.card.unwrap().card.id, card.card.id);
}

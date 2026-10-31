use super::*;

#[test]
fn ulids_are_26_crockford_chars_sorted_by_time() {
    let id = ulid();
    assert_eq!(id.len(), 26);
    assert!(id.bytes().all(|b| CROCKFORD.contains(&b)));
    let ids: Vec<Id> = (0..1000).map(|_| ulid()).collect();
    assert!(ids.windows(2).all(|w| w[0] < w[1]), "monotonic");
    assert!(ulid_from(1, u128::MAX) < ulid_from(2, 0));
    assert_eq!(ulid_from(0, 0), "00000000000000000000000000");
    // The ULID spec's maximum timestamp.
    assert_eq!(&ulid_from(0xFFFF_FFFF_FFFF, 0)[..10], "7ZZZZZZZZZ");
}

#[test]
fn sides_split_on_lines_that_are_exactly_three_dashes() {
    assert_eq!(sides("What is the capital of France?\n---\nParis\n"), ["What is the capital of France?", "Paris"]);
    assert_eq!(sides("a\r\n---\r\nb\r\n"), ["a", "b"]);
    assert_eq!(sides("one-sided note"), ["one-sided note"]);
    assert_eq!(sides("a\n --- \nb"), ["a\n --- \nb"]);
    assert_eq!(sides("a\n----\nb"), ["a\n----\nb"]);
    assert_eq!(sides("q\n---\n\n  code\n\n---\nmore"), ["q", "  code", "more"]);
    assert_eq!(sides(""), [""]);
}

#[test]
fn tags_are_normalized() {
    assert_eq!(normalize_tags([" Spanish", "#verbs", "spanish", "", "#", "B"]), ["b", "spanish", "verbs"]);
}

#[test]
fn json_matches_the_contract() {
    let card = Card {
        id: "C".into(),
        deck_id: "D".into(),
        content: "q\n---\na".into(),
        tags: vec!["t".into()],
        created_at: 1,
        updated_at: 2,
        deleted: false,
    };
    let view = CardView::new(card.clone(), recall_srs::Schedule::new(1).into());
    let json = serde_json::to_value(&view).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "id": "C", "deck_id": "D", "content": "q\n---\na", "tags": ["t"], "created_at": 1, "updated_at": 2,
            "deleted": false, "sides": ["q", "a"],
            "schedule": { "state": "new", "due": 1, "stability": 0.0, "difficulty": 0.0, "reps": 0, "lapses": 0,
                          "last_review": null }
        })
    );
    assert_eq!(serde_json::from_value::<CardView>(json).unwrap(), view);

    let change = Change::Card(card);
    let json = serde_json::to_value(&change).unwrap();
    assert_eq!(json["kind"], "card");
    assert_eq!(json["value"]["deck_id"], "D");
    assert_eq!(serde_json::from_value::<Change>(json).unwrap(), change);
}

use std::io::Write;

use super::value::parse_iso;
use super::*;
use crate::Store;
use crate::model::State;
use crate::model::ulid;

/// Compact Transit as Mochi writes it: `^N` stands for the Nth cacheable string seen so far.
const TRANSIT: &str = r#"["^ ",
  "~:version", 2,
  "~:decks", [
    ["^ ", "~:id", "~:spanish1", "~:name", "Spanish",
      "~:cards", ["~#list", [
        ["^ ", "~:content", "hola\n---\nhello", "~:pos", "b", "~:tags", ["~#set", ["Vocab"]],
          "~:reviews", [["^ ", "~:date", "~t2024-01-01T00:00:00.000Z", "~:remembered?", true]]],
        ["^ ", "^7", "adiós\n---\ngoodbye", "^8", "a"]
      ]]],
    ["^ ", "^2", "~:verbs123", "^4", "Verbs", "~:parent-id", "^3"],
    ["^ ", "^2", "~:trash1234", "^4", "Old", "~:trashed?", "~m1700000000000", "^5", ["^6", [["^ ", "^7", "gone"]]]]
  ],
  "^5", [
    ["^ ", "^7", "top level", "~:deck-id", "^>"],
    ["^ ", "^7", "orphan", "^B", "~:nowhere1"]
  ]
]"#;

#[test]
fn compact_transit_reads_with_its_cache() {
    let lib = library(&transit::parse(TRANSIT).unwrap(), 0).unwrap();
    let names: Vec<&str> = lib.decks.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["Spanish", "Verbs"], "the trashed deck is left out");
    assert_eq!(lib.skipped, 2, "the trashed deck's card and the card whose deck isn't there");
    let spanish = &lib.decks[0];
    let fronts: Vec<&str> = spanish.cards.iter().map(|c| c.content.as_str()).collect();
    assert_eq!(fronts, ["adiós\n---\ngoodbye", "hola\n---\nhello"], "in Mochi's order");
    assert_eq!(spanish.cards[1].tags, ["Vocab"]);
    let review = spanish.cards[1].reviews[0];
    assert_eq!((review.reviewed_at, review.rating), (1_704_067_200_000, REMEMBERED));
    assert_eq!(lib.decks[1].parent.as_deref(), Some("spanish1"));
    assert_eq!(lib.decks[1].cards[0].content, "top level");
}

/// A card as Mochi itself stores it: `~t` with Unix ms, and `created-at` as a `dt` tag.
#[test]
fn mochis_own_dates_read() {
    let json = r#"{"~:version": 2, "~:decks": [{"~:id": "~:unLtfyTC", "~:name": "Chinese"}],
      "~:cards": [{"~:content": "中文\n---\nzhōng wén\nChinese", "~:deck-id": "~:unLtfyTC",
        "~:created-at": {"~#dt": 1774865865456},
        "~:reviews": [{"~:date": "~t1774800000000", "~:due": "~t1774800000000", "~:remembered?": true}]}]}"#;
    let lib = library(&transit::parse(json).unwrap(), 0).unwrap();
    let card = &lib.decks[0].cards[0];
    assert_eq!(card.created_at, Some(1_774_865_865_456));
    assert_eq!(card.reviews[0].reviewed_at, 1_774_800_000_000);
}

#[test]
fn verbose_transit_reads_too() {
    let json =
        r#"{"~:version": 2, "~:decks": [{"~:name": "Plain", "~:cards": {"~#list": [{"~:content": "a\n---\nb"}]}}]}"#;
    let lib = library(&transit::parse(json).unwrap(), 0).unwrap();
    assert_eq!(lib.decks[0].name, "Plain");
    assert_eq!(lib.decks[0].cards[0].content, "a\n---\nb");
}

const EDN: &str = r##"{:version 2
 ; an older export
 :templates [{:id :tmpl0001 :name "Vocab"
              :content "# << Word >>\n---\n<<meaning>>\n<<#note>>Note: <<note>><</note>><<^note>>(no note)<</note>>"
              :fields {:word0001 {:id :word0001 :name "Word"}
                       :mean0001 {:id :mean0001 :name "Meaning"}
                       :note0001 {:id :note0001 :name "note"}}}]
 :decks [{:id :deck0001 :name "Words"
          :cards [{:content "" :template-id :tmpl0001
                   :fields {:word0001 {:id :word0001 :value "gato"} :mean0001 {:id :mean0001 :value "cat"}}
                   :reviews [{:date #inst "2024-02-01T10:00:00.000-00:00" :remembered? false :duration 4}]}
                  #_{:content "discarded"}
                  {:content "  "}]}]}"##;

#[test]
fn edn_templates_render_with_their_fields() {
    let lib = library(&edn::parse(EDN).unwrap(), 0).unwrap();
    let card = &lib.decks[0].cards[0];
    assert_eq!(card.content, "# gato\n---\ncat\n(no note)");
    assert_eq!(
        card.reviews,
        [ImportReview { reviewed_at: 1_706_781_600_000, rating: FORGOT, duration_ms: Some(4000) }]
    );
    assert_eq!(lib.skipped, 1, "the blank card");
}

#[test]
fn sections_show_when_their_field_is_filled() {
    let field = |name: &str| (name.trim() == "x").then(|| "1".to_string());
    assert_eq!(render("<<#x>>yes <<x>><</x>><<^x>>no<</x>>", &field), "yes 1");
    assert_eq!(render("<<#y>>yes<</y>><<^ y >>no<</y>>!", &field), "no!");
    assert_eq!(render("<< missing >> and <<unclosed", &field), " and <<unclosed");
}

#[test]
fn trailing_inline_tags_become_tags() {
    let mut tags = vec!["word".to_string()];
    let content = take_inline_tags("中文\n---\nzhōng wén\nChinese\n\n#word #hsk-1\n", &mut tags);
    assert_eq!(content, "中文\n---\nzhōng wén\nChinese");
    assert_eq!(tags, ["word", "word", "hsk-1"]);
    let mut none = Vec::new();
    assert_eq!(take_inline_tags("# Heading\n---\nback #inline", &mut none), "# Heading\n---\nback #inline");
    assert!(none.is_empty());
}

#[test]
fn iso_dates() {
    assert_eq!(parse_iso("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(parse_iso("2024-01-01T01:00:00+01:00"), Some(1_704_067_200_000));
    assert_eq!(parse_iso("2024-01-01T00:00:00.5Z"), Some(1_704_067_200_500));
    assert_eq!(parse_iso("2024-01-01"), Some(1_704_067_200_000));
    assert_eq!(parse_iso("not a date"), None);
}

#[test]
fn a_mochi_zip_counts_its_attachments() {
    let path = std::env::temp_dir().join(format!("furiten-mochi-{}.mochi", ulid()));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("data.json", options).unwrap();
    zip.write_all(TRANSIT.as_bytes()).unwrap();
    zip.start_file("cat.png", options).unwrap();
    zip.write_all(b"not really a png").unwrap();
    zip.finish().unwrap();
    let lib = read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!((lib.decks.len(), lib.attachments), (2, 1));
}

#[test]
fn not_an_export_says_so() {
    assert_eq!(library(&Value::List(vec![]), 0).unwrap_err().code, "invalid_params");
    assert_eq!(transit::parse("{").unwrap_err().code, "invalid_params");
    assert_eq!(edn::parse("{:a").unwrap_err().code, "invalid_params");
}

#[test]
fn importing_adds_decks_with_their_history() {
    let mut store = Store::open_in_memory().unwrap();
    let before = store.decks().unwrap().len();
    let parent = store.create_deck(crate::protocol::NewDeck { name: "Imports".into(), parent_id: None }).unwrap();
    let lib = library(&transit::parse(TRANSIT).unwrap(), 3).unwrap();
    let imported = store.import(lib, Some(&parent.id)).unwrap();
    assert_eq!((imported.cards, imported.reviews, imported.skipped, imported.attachments), (3, 1, 2, 3));
    let [spanish, verbs] = &imported.decks[..] else { panic!("two decks") };
    assert_eq!(spanish.parent_id.as_ref(), Some(&parent.id));
    assert_eq!(verbs.parent_id.as_ref(), Some(&spanish.id));
    assert_eq!(store.decks().unwrap().len(), before + 3);

    let cards = store.cards(&crate::protocol::CardQuery { deck_id: Some(spanish.id.clone()), ..Default::default() });
    let cards = cards.unwrap();
    let hola = cards.iter().find(|c| c.front() == "hola").unwrap();
    assert_eq!(hola.card.tags, ["vocab"]);
    assert_eq!((hola.schedule.state, hola.schedule.reps), (State::Learning, 1), "scheduled from Mochi's review");
    assert!(hola.schedule.due < 1_704_153_600_000, "due when a review in January 2024 put it");
    assert!(hola.card.created_at <= 1_704_067_200_000, "created before its first review");
    let adios = cards.iter().find(|c| c.front() == "adiós").unwrap();
    assert_eq!(adios.schedule.state, State::New);
}

//! The card being written: one text field per side. A line with only `---` splits a side in two where it stands, and
//! Backspace at the start of a side joins it to the one before, so the fields always match the card's sides.

use rune::TextEdit;

use crate::core::{Id, content_from_sides, sides};

/// How many recently used tags the menu offers before anything is typed.
pub const RECENT_TAGS: usize = 8;

pub struct Editor {
    /// `None` while writing a new card.
    pub card: Option<Id>,
    /// In the order they're revealed; at least one. What goes on each is the writer's (or, later, a template's) call.
    pub sides: Vec<Side>,
    next_side: u32,
    /// The card's tags, in the order they were added, each once.
    pub tags: Vec<String>,
    /// The tag being typed after the chips; a comma, space or Return makes it a chip.
    pub tag_draft: TextEdit,
    /// Every tag in the library, the most recently used first: what the tag menu suggests from.
    pub known_tags: Vec<String>,
    /// The highlighted row of the tag menu, an index into `tag_suggestions`. While something's typed it starts on
    /// the first match, so Return fills it in; with nothing typed, nothing is until ↓, ↑ or the pointer picks one.
    pub tag_active: Option<usize>,
    /// Escape closed the tag menu; typing or an arrow opens it again, and so does focusing the field anew.
    pub tag_menu_closed: bool,
    pub deck: Id,
    pub picking_deck: bool,
    /// The first press on "Delete card" asks again in place.
    pub confirm_delete: bool,
    pub error: Option<String>,
    /// Editing the card under review where it is (E), rather than in the dialog.
    pub inline: bool,
}

pub struct Side {
    /// Stays with the side as others are added or joined, so its field keeps its place.
    pub id: u32,
    pub edit: TextEdit,
}

impl Editor {
    /// Writing `content` (empty for a new card: one empty side), with the caret at the end of the first side.
    pub fn new(card: Option<Id>, content: &str, tags: Vec<String>, deck: Id) -> Editor {
        let mut e = Editor {
            card,
            sides: Vec::new(),
            next_side: 0,
            tags: Vec::new(),
            tag_draft: TextEdit::new(""),
            known_tags: Vec::new(),
            tag_active: None,
            tag_menu_closed: false,
            deck,
            picking_deck: false,
            confirm_delete: false,
            error: None,
            inline: false,
        };
        for text in sides(content) {
            e.push(text);
        }
        for tag in tags {
            e.add_tag(&tag);
        }
        let first = e.sides[0].id;
        let end = e.sides[0].edit.text.len();
        e.focus(first, end);
        e
    }

    fn push(&mut self, text: String) -> u32 {
        self.insert(self.sides.len(), text)
    }

    fn insert(&mut self, at: usize, text: String) -> u32 {
        let id = self.next_side;
        self.next_side += 1;
        let mut edit = TextEdit::new(text);
        edit.set_caret(0);
        self.sides.insert(at, Side { id, edit });
        id
    }

    /// The card's Markdown: the sides joined by `---` lines, without empty ones.
    pub fn content(&self) -> String {
        let texts: Vec<&str> = self.sides.iter().map(|s| s.edit.text.as_str()).collect();
        content_from_sides(&texts)
    }

    pub fn index(&self, id: u32) -> Option<usize> {
        self.sides.iter().position(|s| s.id == id)
    }

    pub fn side_mut(&mut self, id: u32) -> Option<&mut TextEdit> {
        self.sides.iter_mut().find(|s| s.id == id).map(|s| &mut s.edit)
    }

    /// Puts the caret in side `id` at byte `at`, and takes it out of every other field.
    pub fn focus(&mut self, id: u32, at: usize) {
        self.tag_draft.focused = false;
        for s in &mut self.sides {
            s.edit.focused = s.id == id;
            if s.id == id {
                s.edit.set_caret(floor_char(&s.edit.text, at));
            }
        }
    }

    fn add_tag(&mut self, tag: &str) {
        push_tag(&mut self.tags, tag);
    }

    /// What the tag menu offers: tags in use that the card hasn't got, the most recently used first, at most
    /// `RECENT_TAGS` of them. With something typed, every tag containing it, those starting with it first.
    pub fn tag_suggestions(&self) -> Vec<&str> {
        let typed = self.tag_draft.text.trim().trim_start_matches('#').to_lowercase();
        let mut found: Vec<&str> = self
            .known_tags
            .iter()
            .map(String::as_str)
            .filter(|t| !self.tags.iter().any(|c| c.eq_ignore_ascii_case(t)))
            .filter(|t| t.contains(&typed))
            .collect();
        if typed.is_empty() {
            found.truncate(RECENT_TAGS);
        } else {
            found.sort_by_key(|t| !t.starts_with(&typed));
        }
        found
    }

    /// Whether the tag menu shows: the field has the caret, Escape hasn't closed it and there's something to offer.
    pub fn tag_menu_open(&self) -> bool {
        self.tag_draft.focused && !self.tag_menu_closed && !self.tag_suggestions().is_empty()
    }

    /// ↓ or ↑ in the tag field: moves the menu's highlight, round from either end, opening it if Escape closed it.
    /// False when there's nothing to suggest.
    pub fn move_tag_active(&mut self, delta: isize) -> bool {
        let n = self.tag_suggestions().len() as isize;
        if n == 0 {
            return false;
        }
        self.tag_menu_closed = false;
        let next = match self.tag_active {
            Some(i) => (i as isize + delta).rem_euclid(n),
            None if delta > 0 => 0,
            None => n - 1,
        };
        self.tag_active = Some(next as usize);
        true
    }

    /// A suggestion chosen (Return on the highlight, or a click): it becomes a chip in place of what was typed.
    pub fn choose_tag(&mut self, tag: &str) {
        self.add_tag(tag);
        self.tag_draft.clear();
        self.tag_active = None;
    }

    /// Return in the tag field: the highlighted suggestion if there is one, else what's typed. False when neither.
    pub fn enter_tag(&mut self) -> bool {
        let chosen = match (self.tag_menu_open(), self.tag_active) {
            (true, Some(i)) => self.tag_suggestions().get(i).map(|t| t.to_string()),
            _ => None,
        };
        match chosen {
            Some(tag) => {
                self.choose_tag(&tag);
                true
            }
            None => self.commit_tag_draft(),
        }
    }

    /// Escape in the tag field closes the menu. False when it wasn't open, so Escape goes on to cancel the editor.
    pub fn close_tag_menu(&mut self) -> bool {
        let open = self.tag_menu_open();
        self.tag_menu_closed = true;
        self.tag_active = None;
        open
    }

    /// After the draft changes: every tag a comma, space or line break has ended becomes a chip, and the draft keeps
    /// what follows the last one. Typing `a,` makes a chip of `a`; pasting `a, b c` makes `a` and `b` and leaves `c`.
    /// The menu filters to what's left, its first match highlighted (nothing, when nothing's left typed).
    pub fn take_typed_tags(&mut self) {
        self.tag_menu_closed = false;
        let text = &self.tag_draft.text;
        if let Some(last) = text.rfind([',', ' ', '\n']) {
            let (done, rest) = (text[..last].to_owned(), text[last + 1..].to_owned());
            for tag in done.split([',', ' ', '\n']) {
                self.add_tag(tag);
            }
            self.tag_draft.replace_text(rest);
        }
        let searching = !self.tag_draft.text.trim().is_empty() && !self.tag_suggestions().is_empty();
        self.tag_active = searching.then_some(0);
    }

    /// Return in the draft: it becomes a chip. False when it's empty.
    pub fn commit_tag_draft(&mut self) -> bool {
        if self.tag_draft.text.trim().is_empty() {
            return false;
        }
        let draft = std::mem::take(&mut self.tag_draft.text);
        self.add_tag(&draft);
        self.tag_draft.clear();
        true
    }

    pub fn remove_tag(&mut self, tag: &str) {
        self.tags.retain(|t| t != tag);
    }

    /// Backspace in an empty draft takes the last chip off. False when there's no chip or the draft isn't empty.
    pub fn pop_tag(&mut self) -> bool {
        let popped = self.tag_draft.text.is_empty() && self.tags.pop().is_some();
        if popped {
            self.tag_active = None;
        }
        popped
    }

    /// What saves: the chips, and the draft if something's still typed there.
    pub fn saved_tags(&self) -> Vec<String> {
        let mut tags = self.tags.clone();
        push_tag(&mut tags, &self.tag_draft.text);
        tags
    }

    /// Return in side `id`: a new line, which splits the side if it ends a `---` line.
    ///
    /// In a list item it continues the list: the next line starts with the same indent and marker (the next number in a
    /// numbered list, an empty box in a task list). Return on an item with nothing after its marker ends the list
    /// instead, taking the marker off.
    pub fn newline(&mut self, id: u32) {
        if let Some(edit) = self.side_mut(id) {
            let at = edit.focus.min(edit.anchor);
            let start = edit.text[..at].rfind('\n').map_or(0, |i| i + 1);
            let end = edit.text[at..].find('\n').map_or(edit.text.len(), |i| at + i);
            let (range, insert) = match list_item(&edit.text[start..end]) {
                Some(item) if item.body_empty => (Some(start..end), item.indent.to_owned()),
                Some(item) if at >= start + item.marker_end => (None, format!("\n{}{}", item.indent, item.next)),
                _ => (None, "\n".to_owned()),
            };
            if let Some(r) = range {
                edit.set_selection(r.start, r.end);
            }
            edit.insert(&insert);
        }
        self.split();
    }

    /// Splits every side at its `---` lines, except the one the caret is on, which may still be being typed. The caret
    /// follows its text into whichever side that lands in.
    pub fn split(&mut self) {
        let mut i = 0;
        while i < self.sides.len() {
            let edit = &self.sides[i].edit;
            let caret = edit.focused.then_some(edit.focus);
            let Some(line) = separator(&edit.text, caret) else {
                i += 1;
                continue;
            };
            let text = edit.text.clone();
            let before = text[..line.start].strip_suffix('\n').unwrap_or(&text[..line.start]).to_string();
            let rest = (line.end + 1).min(text.len());
            let after = text[rest..].to_string();
            let id = self.sides[i].id;
            let edit = &mut self.sides[i].edit;
            edit.replace_text(&before);
            let new = self.insert(i + 1, after);
            match caret {
                Some(at) if at >= rest => self.focus(new, at - rest),
                Some(at) => self.focus(id, at.min(before.len())),
                None => {}
            }
            i += 1;
        }
    }

    /// Backspace at the very start of side `id`: joins it to the end of the side before (a new line between, when
    /// both have text), with the caret where they meet. False when there's nothing before it to join to.
    pub fn join_previous(&mut self, id: u32) -> bool {
        let Some(i) = self.index(id).filter(|&i| i > 0) else { return false };
        let side = self.sides.remove(i);
        let prev = &mut self.sides[i - 1];
        let mut text = prev.edit.text.clone();
        let at = text.len();
        if !text.is_empty() && !side.edit.text.is_empty() {
            text.push('\n');
        }
        text.push_str(&side.edit.text);
        prev.edit.replace_text(&text);
        let prev = prev.id;
        self.focus(prev, at);
        true
    }

    /// ⇧Return or ⌘- in side `id`: splits it at the caret (replacing what's selected), what follows starting the
    /// next side with the caret at its start.
    pub fn new_side_at(&mut self, id: u32) {
        if let Some(edit) = self.side_mut(id) {
            edit.insert("\n---\n");
        }
        self.split();
    }

    /// ⌘B (`**`) or ⌘I (`*`) in side `id`: wraps the selection in `mark`, or unwraps it when it's wrapped already.
    /// With nothing selected, the caret lands between a new pair.
    pub fn toggle_mark(&mut self, id: u32, mark: &str) {
        let Some(edit) = self.side_mut(id) else { return };
        let (a, b) = (edit.anchor.min(edit.focus), edit.anchor.max(edit.focus));
        let (t, n) = (&edit.text, mark.len());
        let before = t[..a].len() - t[..a].trim_end_matches('*').len();
        let after = t[b..].len() - t[b..].trim_start_matches('*').len();
        // Italic's `*` is the odd one out of a run (`*`, or the third of `***`); a bold `**` alone isn't it.
        let wrapped = if n == 1 { before % 2 == 1 && after % 2 == 1 } else { before >= n && after >= n };
        let inner = t[a..b].to_string();
        if wrapped {
            edit.set_selection(a - n, b + n);
            edit.insert(&inner);
            edit.set_selection(a - n, b - n);
        } else {
            edit.insert(&format!("{mark}{inner}{mark}"));
            edit.set_selection(a + n, b + n);
        }
    }

    /// A new empty side at the end, with the caret in it. An empty last side is reused.
    pub fn add_side(&mut self) {
        let last = self.sides.last().filter(|s| s.edit.text.is_empty()).map(|s| s.id);
        let id = last.unwrap_or_else(|| self.push(String::new()));
        self.focus(id, 0);
    }

    /// The side before or after `id`, for the arrows at either end of a field.
    pub fn neighbour(&self, id: u32, step: isize) -> Option<u32> {
        let i = self.index(id)?.checked_add_signed(step)?;
        self.sides.get(i).map(|s| s.id)
    }
}

/// A Markdown list item's line, as Return sees it.
struct ListItem<'a> {
    /// The spaces or tabs before its marker.
    indent: &'a str,
    /// Where its text starts: past the indent, the marker, its space and any task box.
    marker_end: usize,
    /// The marker the next item starts with: the same bullet, the next number, an empty task box.
    next: String,
    /// Nothing follows the marker.
    body_empty: bool,
}

/// `line` as a list item: `- `, `* ` or `+ `, `1. ` or `1) `, each optionally followed by a task box `[ ] `.
fn list_item(line: &str) -> Option<ListItem<'_>> {
    let line = line.trim_end_matches('\r');
    let indent = &line[..line.len() - line.trim_start_matches([' ', '\t']).len()];
    let rest = &line[indent.len()..];
    let (marker, next) = if let Some(b) = rest.chars().next().filter(|c| matches!(c, '-' | '*' | '+')) {
        (1, b.to_string())
    } else {
        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        let delim = rest[digits..].chars().next().filter(|c| matches!(c, '.' | ')'));
        match (digits, delim) {
            (1..=9, Some(d)) => (digits + 1, format!("{}{d}", rest[..digits].parse::<u64>().ok()? + 1)),
            _ => return None,
        }
    };
    // A marker takes a space after it (or ends the line, as an item still empty does); `**bold**` and `---` aren't items.
    let after = &rest[marker..];
    if !(after.is_empty() || after.starts_with(' ')) {
        return None;
    }
    let mut body = after.strip_prefix(' ').unwrap_or(after);
    let mut next = format!("{next} ");
    for task in ["[ ]", "[x]", "[X]"] {
        if let Some(b) = body.strip_prefix(task).filter(|b| b.is_empty() || b.starts_with(' ')) {
            body = b.strip_prefix(' ').unwrap_or(b);
            next.push_str("[ ] ");
            break;
        }
    }
    Some(ListItem { indent, marker_end: line.len() - body.len(), next, body_empty: body.trim().is_empty() })
}

/// Adds `tag` to `tags` (a leading `#` dropped) unless it's empty or already there.
fn push_tag(tags: &mut Vec<String>, tag: &str) {
    let tag = tag.trim().trim_start_matches('#');
    if !tag.is_empty() && !tags.iter().any(|t| t == tag) {
        tags.push(tag.to_owned());
    }
}

/// The first line of `text` that is exactly `---`, other than the one holding `caret`.
fn separator(text: &str, caret: Option<usize>) -> Option<std::ops::Range<usize>> {
    let mut start = 0;
    for line in text.split('\n') {
        let end = start + line.len();
        let holds_caret = caret.is_some_and(|c| c >= start && c <= end);
        if line.trim_end_matches('\r') == "---" && !holds_caret {
            return Some(start..end);
        }
        start = end + 1;
    }
    None
}

fn floor_char(s: &str, mut at: usize) -> usize {
    at = at.min(s.len());
    while !s.is_char_boundary(at) {
        at -= 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor(content: &str) -> Editor {
        Editor::new(None, content, Vec::new(), "deck".into())
    }

    fn texts(e: &Editor) -> Vec<&str> {
        e.sides.iter().map(|s| s.edit.text.as_str()).collect()
    }

    fn focused(e: &Editor) -> Option<(usize, usize)> {
        e.sides.iter().enumerate().find(|(_, s)| s.edit.focused).map(|(i, s)| (i, s.edit.focus))
    }

    fn typed(e: &mut Editor, text: &str) {
        e.tag_draft.replace_text(text);
        e.take_typed_tags();
    }

    #[test]
    fn typing_a_separator_makes_a_chip() {
        let mut e = editor("");
        typed(&mut e, "vocab,");
        typed(&mut e, "#food ");
        assert_eq!((e.tags.clone(), e.tag_draft.text.as_str()), (vec!["vocab".to_owned(), "food".to_owned()], ""));
        typed(&mut e, "a, b vocab c");
        assert_eq!(e.tags, ["vocab", "food", "a", "b"], "a paste splits, and a tag is only there once");
        assert_eq!(e.tag_draft.text, "c", "what follows the last separator is still being typed");
        assert_eq!(e.saved_tags(), ["vocab", "food", "a", "b", "c"], "a typed tag saves without Return");
    }

    #[test]
    fn return_commits_and_backspace_takes_the_last_chip() {
        let mut e = Editor::new(None, "", vec!["vocab".into()], "deck".into());
        assert!(!e.commit_tag_draft(), "nothing to commit");
        e.tag_draft.replace_text("food");
        assert!(!e.pop_tag(), "Backspace edits the draft while it has text");
        assert!(e.commit_tag_draft());
        assert_eq!(e.tags, ["vocab", "food"]);
        assert!(e.pop_tag());
        assert_eq!(e.tags, ["vocab"]);
        e.remove_tag("vocab");
        assert!(e.tags.is_empty() && !e.pop_tag());
    }

    /// The side's text after Return with the caret at the end of `text`.
    fn enter_at_end(text: &str) -> (String, usize) {
        let mut e = editor(text);
        let id = e.sides[0].id;
        e.focus(id, text.len());
        e.newline(id);
        (e.sides[0].edit.text.clone(), e.sides[0].edit.focus)
    }

    #[test]
    fn return_in_a_list_continues_it() {
        assert_eq!(enter_at_end("- bread").0, "- bread\n- ");
        assert_eq!(enter_at_end("  * nested").0, "  * nested\n  * ", "same indent and bullet");
        assert_eq!(enter_at_end("9. ninth").0, "9. ninth\n10. ", "the next number");
        assert_eq!(enter_at_end("1) first").0, "1) first\n2) ");
        assert_eq!(enter_at_end("- [x] done").0, "- [x] done\n- [ ] ", "a fresh box");
        let (text, caret) = enter_at_end("- one");
        assert_eq!(caret, text.len(), "the caret starts the new item");
    }

    #[test]
    fn return_on_an_empty_item_ends_the_list() {
        assert_eq!(enter_at_end("- one\n- ").0, "- one\n");
        assert_eq!(enter_at_end("- one\n  - [ ] ").0, "- one\n  ", "keeps the indent");
        assert_eq!(enter_at_end("1. one\n2.").0, "1. one\n");
    }

    #[test]
    fn return_outside_a_list_is_just_a_line() {
        for text in ["plain", "**bold**", "-not a bullet", "1.5 litres"] {
            assert_eq!(enter_at_end(text).0, format!("{text}\n"), "{text}");
        }
        // Before the marker, Return only opens a line above.
        let mut e = editor("- item");
        let id = e.sides[0].id;
        e.focus(id, 0);
        e.newline(id);
        assert_eq!(e.sides[0].edit.text, "\n- item");
    }

    fn tagging(card_tags: &[&str], known: &[&str]) -> Editor {
        let mut e = Editor::new(None, "", card_tags.iter().map(|t| t.to_string()).collect(), "deck".into());
        e.known_tags = known.iter().map(|t| t.to_string()).collect();
        e.tag_draft.focused = true;
        e
    }

    #[test]
    fn the_tag_menu_offers_recent_tags_the_card_lacks_and_filters_as_you_type() {
        let known: Vec<String> = (0..12).map(|i| format!("t{i}")).collect();
        let known: Vec<&str> = known.iter().map(String::as_str).collect();
        let e = tagging(&["T1"], &known);
        assert_eq!(e.tag_suggestions(), ["t0", "t2", "t3", "t4", "t5", "t6", "t7", "t8"], "recent first, capped");

        let mut e = tagging(&[], &["verbs", "food", "vocab", "spanish"]);
        typed(&mut e, "#V");
        assert_eq!(e.tag_suggestions(), ["verbs", "vocab"], "case and a leading # don't matter");
        typed(&mut e, "s");
        assert_eq!(e.tag_suggestions(), ["spanish", "verbs"], "starting with it, then containing it, each by recency");
        typed(&mut e, "zzz");
        assert!(e.tag_suggestions().is_empty() && !e.tag_menu_open(), "nothing to offer hides the menu");
        e.tag_draft.focused = false;
        typed(&mut e, "");
        assert!(!e.tag_menu_open(), "only while the field has the caret");
    }

    #[test]
    fn arrows_highlight_a_suggestion_and_return_adds_it() {
        let mut e = tagging(&[], &["verbs", "food", "vocab"]);
        assert_eq!(e.tag_active, None, "nothing typed, nothing highlighted");
        assert!(e.move_tag_active(-1));
        assert_eq!(e.tag_active, Some(2), "↑ from nothing starts at the bottom");
        assert!(e.move_tag_active(1));
        assert_eq!(e.tag_active, Some(0), "and goes round");

        typed(&mut e, "v");
        assert_eq!(e.tag_active, Some(0), "typing highlights the first match");
        assert!(e.move_tag_active(1));
        assert!(e.enter_tag());
        assert_eq!((e.tags.clone(), e.tag_draft.text.as_str()), (vec!["vocab".to_owned()], ""));
        assert_eq!(e.tag_active, None);
        assert_eq!(e.tag_suggestions(), ["verbs", "food"], "a chosen tag leaves the menu");
        assert!(e.tag_draft.focused, "the field keeps the caret for the next tag");

        typed(&mut e, "ver");
        assert!(e.enter_tag(), "Return fills in the first match");
        assert_eq!(e.tags, ["vocab", "verbs"]);

        typed(&mut e, "fo");
        e.close_tag_menu();
        assert!(e.enter_tag(), "with the menu closed, Return adds what's typed");
        typed(&mut e, "zz");
        assert!(e.enter_tag(), "and with no match");
        assert_eq!(e.tags, ["vocab", "verbs", "fo", "zz"]);
    }

    #[test]
    fn escape_closes_the_tag_menu_until_you_type() {
        let mut e = tagging(&[], &["verbs"]);
        assert!(e.tag_menu_open());
        assert!(e.close_tag_menu());
        assert!(!e.tag_menu_open());
        assert!(!e.close_tag_menu(), "a second Escape is the editor's");
        typed(&mut e, "v");
        assert!(e.tag_menu_open());
        e.close_tag_menu();
        assert!(e.move_tag_active(1) && e.tag_menu_open(), "an arrow opens it too");
    }

    #[test]
    fn a_new_card_is_one_empty_side() {
        let e = editor("");
        assert_eq!(texts(&e), [""]);
        assert_eq!(focused(&e), Some((0, 0)));
        assert_eq!(e.content(), "");
    }

    #[test]
    fn sides_load_and_save_as_the_card_has_them() {
        let e = editor("苹果\n---\npíng guǒ\napples\n---\nfruit");
        assert_eq!(texts(&e), ["苹果", "píng guǒ\napples", "fruit"]);
        assert_eq!(focused(&e), Some((0, "苹果".len())));
        assert_eq!(e.content(), "苹果\n---\npíng guǒ\napples\n---\nfruit");
        assert_eq!(editor("just a note").content(), "just a note");
    }

    #[test]
    fn return_is_a_new_line() {
        let mut e = editor("one");
        let first = e.sides[0].id;
        e.newline(first);
        assert_eq!(texts(&e), ["one\n"]);
        assert_eq!(focused(&e), Some((0, 4)));
    }

    #[test]
    fn return_after_dashes_splits_the_side_there() {
        let mut e = editor("苹果\n---\npíng guǒ\n---\napples");
        let second = e.sides[1].id;
        // "píng guǒ\n---" with the caret after the dashes, then Return.
        let edit = e.side_mut(second).unwrap();
        edit.replace_text("píng guǒ\n---\nmore");
        edit.set_caret("píng guǒ\n---".len());
        e.focus(second, "píng guǒ\n---".len());
        e.newline(second);
        assert_eq!(texts(&e), ["苹果", "píng guǒ", "\nmore", "apples"]);
        assert_eq!(focused(&e), Some((2, 0)), "the caret starts the new side");
    }

    #[test]
    fn dashes_still_being_typed_dont_split() {
        let mut e = editor("q");
        let first = e.sides[0].id;
        e.side_mut(first).unwrap().replace_text("q\n---");
        e.focus(first, 5);
        e.split();
        assert_eq!(texts(&e), ["q\n---"]);
    }

    #[test]
    fn a_pasted_card_splits_into_its_sides() {
        let mut e = editor("");
        let first = e.sides[0].id;
        e.side_mut(first).unwrap().replace_text("q\n---\na\n---\nmore");
        e.focus(first, "q\n---\na\n---\nmore".len());
        e.split();
        assert_eq!(texts(&e), ["q", "a", "more"]);
        assert_eq!(focused(&e), Some((2, 4)));
        assert_eq!(e.content(), "q\n---\na\n---\nmore");
    }

    #[test]
    fn backspace_at_the_start_joins_the_side_before() {
        let mut e = editor("q\n---\na\n---\nmore");
        let (second, third) = (e.sides[1].id, e.sides[2].id);
        assert!(e.join_previous(third));
        assert_eq!(texts(&e), ["q", "a\nmore"]);
        assert_eq!(focused(&e), Some((1, 1)));
        assert!(e.join_previous(second));
        assert_eq!(texts(&e), ["q\na\nmore"]);
        let first = e.sides[0].id;
        assert!(!e.join_previous(first), "nothing before the first side");
    }

    #[test]
    fn a_new_side_starts_at_the_caret() {
        let mut e = editor("question answer\n---\nmore");
        let first = e.sides[0].id;
        e.focus(first, "question".len());
        e.new_side_at(first);
        assert_eq!(texts(&e), ["question", " answer", "more"]);
        assert_eq!(focused(&e), Some((1, 0)));
        let last = e.sides[2].id;
        e.focus(last, 4);
        e.new_side_at(last);
        assert_eq!(texts(&e), ["question", " answer", "more", ""], "at the end, an empty side");
    }

    #[test]
    fn marks_wrap_the_selection_and_come_off_again() {
        let mut e = editor("a word here");
        let id = e.sides[0].id;
        e.side_mut(id).unwrap().set_selection(2, 6);
        e.toggle_mark(id, "**");
        assert_eq!(texts(&e), ["a **word** here"]);
        let edit = e.side_mut(id).unwrap();
        assert_eq!((edit.anchor, edit.focus), (4, 8), "the word stays selected");
        e.toggle_mark(id, "*");
        assert_eq!(texts(&e), ["a ***word*** here"], "italic inside bold isn't bold's to take off");
        e.toggle_mark(id, "*");
        e.toggle_mark(id, "**");
        assert_eq!(texts(&e), ["a word here"]);

        let mut e = editor("ab");
        let id = e.sides[0].id;
        e.focus(id, 1);
        e.toggle_mark(id, "**");
        assert_eq!(texts(&e), ["a****b"]);
        assert_eq!(e.side_mut(id).unwrap().focus, 3, "the caret between the pair");
    }

    #[test]
    fn adding_a_side_reuses_an_empty_last_one() {
        let mut e = editor("q");
        e.add_side();
        assert_eq!((texts(&e), focused(&e)), (vec!["q", ""], Some((1, 0))));
        e.add_side();
        assert_eq!((texts(&e), focused(&e)), (vec!["q", ""], Some((1, 0))), "the empty one is reused");
    }
}

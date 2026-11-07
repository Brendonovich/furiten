//! Markdown styled in place while you write it: bold reads bold, a heading reads large, code reads in mono on a tint.
//! The marks that do it (`**`, `#`, backticks, a link's URL) stay in the text, faded, on the lines the caret or
//! selection is on, and are hidden everywhere else, so the rest of the card reads as it will render. A bullet item's `-`
//! always gives way to a drawn bullet ([`blocks`]), on the caret's line too: it reads the same either way. Numbers, task
//! boxes, rules and HTML always show, faded, as they read the same rendered. It parses with pulldown-cmark, as the Markdown the cards render with does, so
//! what's styled is what will render.

use std::ops::Range;

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use rune::{Block, Color, Span};
use rune_kit::Kit;

/// The colours the styling uses.
pub struct Palette {
    /// The marks themselves.
    pub mark: Color,
    /// A link's text.
    pub link: Color,
    /// A quote's and a struck-out run's text.
    pub muted: Color,
    /// Behind inline code and code blocks.
    pub code_bg: Color,
}

impl Palette {
    pub fn from_kit(k: &Kit) -> Palette {
        Palette { mark: k.c.text_faint, link: k.c.text_accent, muted: k.c.text_muted, code_bg: k.c.layer_03 }
    }
}

/// A hidden mark's point size: a text field can't drop characters from its layout, so they shrink to nothing.
const HIDDEN_SIZE: f32 = 0.01;

/// Spans that style `src` (Markdown) in place, for a text field set at `size` points. `shown` is the byte range
/// whose lines show their marks (the caret's or selection's, from [`shown_lines`]); `None` hides them all.
pub fn spans(src: &str, size: f32, p: &Palette, shown: Option<Range<usize>>) -> Vec<Span> {
    let mut out = Vec::new();
    let clear = Color { a: 0.0, ..p.mark };
    let faded = |out: &mut Vec<Span>, r: Range<usize>| {
        if r.start < r.end {
            out.push(Span::new(r.start, r.end).color(p.mark));
        }
    };
    let hide = |r: Range<usize>| {
        // Clear over any tint or underline the run would otherwise draw at its sliver of a size.
        let mut hidden = Span::new(r.start, r.end).size(HIDDEN_SIZE).color(clear);
        hidden.bg = Some(clear);
        hidden.underline = Some(clear);
        hidden
    };
    let mark = |out: &mut Vec<Span>, r: Range<usize>| {
        if r.start >= r.end {
            return;
        }
        if is_shown(&shown, &r) {
            out.push(Span::new(r.start, r.end).color(p.mark));
        } else {
            out.push(hide(r));
        }
    };
    let plain = masked(src);
    for (event, r) in parse(&plain) {
        let body = &src[r.clone()];
        match event {
            Event::Start(Tag::Strong) => {
                out.push(Span::new(r.start, r.end).bold());
                mark(&mut out, r.start..r.start + 2);
                mark(&mut out, r.end - 2..r.end);
            }
            Event::Start(Tag::Emphasis) => {
                out.push(Span::new(r.start, r.end).italic());
                mark(&mut out, r.start..r.start + 1);
                mark(&mut out, r.end - 1..r.end);
            }
            Event::Start(Tag::Strikethrough) => {
                out.push(Span::new(r.start, r.end).color(p.muted));
                let n = body.len() - body.trim_start_matches('~').len();
                mark(&mut out, r.start..r.start + n);
                mark(&mut out, r.end - n.min(body.len())..r.end);
            }
            Event::Code(_) => {
                out.push(Span::new(r.start, r.end).mono().bg(p.code_bg, 2.0));
                let n = body.len() - body.trim_start_matches('`').len();
                mark(&mut out, r.start..r.start + n);
                mark(&mut out, r.end - n.min(body.len())..r.end);
            }
            Event::Start(Tag::Link { .. } | Tag::Image { .. }) => {
                out.push(Span::new(r.start, r.end).color(p.link));
                match body.rfind("](") {
                    // [text](url): the brackets and the URL fade.
                    Some(at) if body.starts_with('[') || body.starts_with("![") => {
                        let open = if body.starts_with('!') { 2 } else { 1 };
                        mark(&mut out, r.start..r.start + open);
                        mark(&mut out, r.start + at..r.end);
                    }
                    _ => out.push(Span::new(r.start, r.end).underline(p.link)),
                }
            }
            Event::Start(Tag::Heading { level, .. }) => {
                let scale = match level {
                    HeadingLevel::H1 => 1.4,
                    HeadingLevel::H2 => 1.25,
                    HeadingLevel::H3 => 1.12,
                    _ => 1.0,
                };
                out.push(Span::new(r.start, r.end).bold().size(size * scale));
                let hashes = body.len() - body.trim_start_matches('#').len();
                if hashes > 0 {
                    let space = body[hashes..].len() - body[hashes..].trim_start().len();
                    mark(&mut out, r.start..r.start + hashes + space);
                } else if let Some(nl) = body.trim_end().rfind('\n') {
                    // A setext heading's `===` or `---` underline: a line of its own, so it stays.
                    faded(&mut out, r.start + nl + 1..r.end);
                }
            }
            Event::Start(Tag::BlockQuote(_)) => {
                out.push(Span::new(r.start, r.end).color(p.muted));
                for (at, line) in lines(body) {
                    let lead = line.len() - line.trim_start().len();
                    if line[lead..].starts_with('>') {
                        // The `>` and the space after it, as a heading's `#`.
                        let space = usize::from(line[lead + 1..].starts_with(' '));
                        mark(&mut out, r.start + at..r.start + at + lead + 1 + space);
                    }
                }
            }
            Event::Start(Tag::Item) => match bullet(src, r.start) {
                // Hidden above, indentation and all, where `blocks` draws a bullet in its place; a task's box stays.
                Some(_) => {}
                _ => {
                    let lead = body.len() - body.trim_start().len();
                    let marker = body[lead..].find(char::is_whitespace).unwrap_or(body.len() - lead);
                    faded(&mut out, r.start + lead..r.start + lead + marker);
                }
            },
            Event::TaskListMarker(_) => faded(&mut out, r),
            Event::Start(Tag::CodeBlock(_)) => {
                out.push(Span::new(r.start, r.end).mono().bg(p.code_bg, 2.0));
                for (at, line) in lines(body) {
                    if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
                        mark(&mut out, r.start + at..r.start + at + line.len());
                    }
                }
            }
            Event::Rule | Event::Html(_) | Event::InlineHtml(_) => faded(&mut out, r),
            _ => {}
        }
    }
    for r in breaks(src).filter(|r| empty_bullet(&src[r.clone()]).is_none()) {
        faded(&mut out, r);
    }
    // Every bullet's marker, the empty ones the parser no longer sees included, hides behind its drawn bullet.
    for (marker, _) in bullets(src) {
        out.push(hide(marker));
    }
    out
}

/// One block per line of `src` for the field to lay out with: a bullet item is a [`Block::Bullet`], indented to its
/// nesting with a bullet drawn before it, while [`spans`] hides its `-`. Every other line is a paragraph.
pub fn blocks(src: &str) -> Vec<Block> {
    let starts: Vec<usize> = lines(src).map(|(at, _)| at).collect();
    let mut out = vec![Block::Paragraph; starts.len()];
    for (marker, depth) in bullets(src) {
        let line = starts.partition_point(|&at| at <= marker.start) - 1;
        out[line] = Block::Bullet(depth.min(rune::MAX_LIST_LEVEL));
    }
    out
}

/// Each bullet item's hidden marker (from its line's start through the `-` and its space), with its nesting depth.
fn bullets(src: &str) -> Vec<(Range<usize>, u8)> {
    let mut out = Vec::new();
    let mut depth = 0u8;
    let plain = masked(src);
    for (event, r) in parse(&plain) {
        match event {
            Event::Start(Tag::List(_)) => depth += 1,
            Event::End(TagEnd::List(_)) => depth = depth.saturating_sub(1),
            Event::Start(Tag::Item) => {
                if let Some(marker) = bullet(src, r.start) {
                    out.push((marker, depth.saturating_sub(1)));
                }
            }
            _ => {}
        }
    }
    // Empty bullets, blanked for the parser: as deep as the bullet before at the same indent, one deeper under a
    // shallower one.
    for (at, line) in lines(src) {
        let Some(end) = empty_bullet(line) else { continue };
        let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
        let depth = out.iter().filter(|(m, _)| m.start < at).max_by_key(|(m, _)| m.start).map_or(0, |(m, d)| {
            let above = src[m.clone()].len() - src[m.clone()].trim_start_matches([' ', '\t']).len();
            match indent.cmp(&above) {
                std::cmp::Ordering::Equal => *d,
                std::cmp::Ordering::Greater => d + 1,
                std::cmp::Ordering::Less => d.saturating_sub(1),
            }
        });
        out.push((at..at + end, depth));
    }
    out.sort_by_key(|(m, _)| m.start);
    out
}

/// The hidden marker of the bullet item on the line holding byte `at`, if that line is one. The editor steps the caret
/// over it, so the caret never goes where nothing is drawn.
pub fn bullet_marker(src: &str, at: usize) -> Option<Range<usize>> {
    let start = src[..at.min(src.len())].rfind('\n').map_or(0, |i| i + 1);
    bullets(src).into_iter().map(|(m, _)| m).find(|m| m.start == start)
}

fn parse(src: &str) -> impl Iterator<Item = (Event<'_>, Range<usize>)> {
    let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS;
    Parser::new_ext(src, opts).into_offset_iter()
}

/// The lines of `src` that are only dashes (see [`dashes_only`]).
fn breaks(src: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    lines(src).filter(|(_, l)| dashes_only(l)).map(|(at, l)| at..at + l.len())
}

/// A line of only dashes, after any indent and before any trailing spaces: a side break as it's typed (`-`, `--`,
/// `---`), or an empty bullet (`- `). Markdown would read any of them under a paragraph as a setext heading's
/// underline, turning the paragraph into a heading.
fn dashes_only(line: &str) -> bool {
    let body = line.trim_end_matches('\r').trim_matches([' ', '\t']);
    !body.is_empty() && body.chars().all(|c| c == '-')
}

/// An empty bullet item, `- ` and nothing after: the marker range, through its spaces.
fn empty_bullet(line: &str) -> Option<usize> {
    let line = line.trim_end_matches('\r');
    let rest = line.trim_start_matches([' ', '\t']).strip_prefix('-')?;
    (!rest.is_empty() && rest.chars().all(|c| c == ' ' || c == '\t')).then_some(line.len())
}

/// `src` with its lines of only dashes blanked, byte for byte, so Markdown doesn't read one as a rule or as a setext
/// heading's underline (`apple` over `-` or `---` would turn the line above into a heading). An empty bullet among
/// them is drawn as a bullet by [`bullets`] all the same.
fn masked(src: &str) -> String {
    let mut out = src.to_owned();
    for r in breaks(src) {
        out.replace_range(r.clone(), &" ".repeat(r.len()));
    }
    out
}

fn is_shown(shown: &Option<Range<usize>>, r: &Range<usize>) -> bool {
    shown.as_ref().is_some_and(|s| r.start <= s.end && s.start <= r.end)
}

/// For a bullet item (`-`, `*` or `+`) starting at `item`, alone at the start of its line, the range from the line's
/// start through the marker and the space after it.
fn bullet(src: &str, item: usize) -> Option<Range<usize>> {
    let start = src[..item].rfind('\n').map_or(0, |i| i + 1);
    if !src[start..item].chars().all(|c| c == ' ' || c == '\t') {
        return None;
    }
    let rest = &src[item..];
    let marker = rest.chars().next().filter(|c| matches!(c, '-' | '*' | '+'))?;
    let after = &rest[marker.len_utf8()..];
    let space = after.len() - after.trim_start_matches([' ', '\t']).len();
    // Only once a space follows: a lone `-` may be the start of a `---` side break.
    (space > 0).then(|| start..item + marker.len_utf8() + space)
}

/// The byte range of the lines holding `edit`'s caret and selection, when it's focused.
pub fn shown_lines(edit: &rune::TextEdit) -> Option<Range<usize>> {
    if !edit.focused {
        return None;
    }
    let (a, b) = (edit.anchor.min(edit.focus), edit.anchor.max(edit.focus));
    let text = &edit.text;
    let start = text[..a.min(text.len())].rfind('\n').map_or(0, |i| i + 1);
    let end = text[b.min(text.len())..].find('\n').map_or(text.len(), |i| b + i);
    Some(start..end)
}

/// Each line of `s` with the byte offset it starts at.
fn lines(s: &str) -> impl Iterator<Item = (usize, &str)> {
    s.split('\n').scan(0, |at, line| {
        let start = *at;
        *at += line.len() + 1;
        Some((start, line))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn grey(v: f32) -> Color {
        Color { r: v, g: v, b: v, a: 1.0 }
    }

    const P: Palette = Palette { mark: grey(0.1), link: grey(0.2), muted: grey(0.3), code_bg: grey(0.4) };

    /// The text each faded span covers.
    fn marks<'a>(src: &'a str, spans: &[Span]) -> Vec<&'a str> {
        spans.iter().filter(|s| s.color == Some(P.mark)).map(|s| &src[s.start as usize..s.end as usize]).collect()
    }

    /// The text each hidden span covers.
    fn hidden<'a>(src: &'a str, spans: &[Span]) -> Vec<&'a str> {
        spans.iter().filter(|s| s.size == Some(HIDDEN_SIZE)).map(|s| &src[s.start as usize..s.end as usize]).collect()
    }

    fn focused(src: &str, caret: usize) -> rune::TextEdit {
        let mut edit = rune::TextEdit::new(src);
        edit.focused = true;
        edit.set_caret(caret);
        edit
    }

    #[test]
    fn marks_fade_and_what_they_mark_is_styled() {
        let src = "# Title\n\n**la** *merienda* `code` [link](https://x.y)\n\n- item\n> quote";
        let spans = spans(src, 16.0, &P, Some(0..src.len()));
        assert_eq!(marks(src, &spans), ["# ", "**", "**", "*", "*", "`", "`", "[", "](https://x.y)", "> "]);
        let bold = spans.iter().find(|s| s.weight.is_some() && s.size.is_none()).unwrap();
        assert_eq!(&src[bold.start as usize..bold.end as usize], "**la**");
        let heading = spans.iter().find(|s| s.size.is_some()).unwrap();
        assert_eq!((heading.start, heading.size), (0, Some(16.0 * 1.4)));
        assert!(spans.iter().any(|s| s.mono && &src[s.start as usize..s.end as usize] == "`code`"));
    }

    #[test]
    fn a_side_break_still_being_typed_is_not_a_heading_underline() {
        let src = "píng guǒ\napple\n---";
        let styled = spans(src, 16.0, &P, None);
        assert!(styled.iter().all(|s| s.weight.is_none() && s.size.is_none()), "no heading: {styled:?}");
        assert_eq!(marks(src, &styled), ["---"], "the break itself fades");
    }

    #[test]
    fn a_dash_typed_under_a_paragraph_is_not_a_heading_underline() {
        for line in ["-", "--", "- "] {
            let src = format!("píng guǒ\napple\n{line}");
            let styled = spans(&src, 16.0, &P, None);
            assert!(styled.iter().all(|s| s.weight.is_none()), "{line:?} made a heading: {styled:?}");
        }
        let src = "apple\n- ";
        assert_eq!(blocks(src), [Block::Paragraph, Block::Bullet(0)], "an empty bullet still draws");
        assert_eq!(hidden(src, &spans(src, 16.0, &P, None)), ["- "]);
        let nested = "- one\n  - two\n  - ";
        assert_eq!(blocks(nested)[2], Block::Bullet(1), "as deep as the bullet above it");
    }

    #[test]
    fn plain_text_has_no_spans() {
        assert!(spans("苹果 píng guǒ", 16.0, &P, None).is_empty());
    }

    #[test]
    fn only_the_caret_line_shows_its_marks() {
        let src = "# Title\n**la** *merienda*\n- item";
        let edit = focused(src, src.find("la").unwrap());
        let spans = spans(src, 16.0, &P, shown_lines(&edit));
        assert_eq!(marks(src, &spans), ["**", "**", "*", "*"], "the caret's line");
        assert_eq!(hidden(src, &spans), ["# ", "- "]);
    }

    #[test]
    fn bullets_draw_as_bullets_even_on_the_caret_line() {
        let src = "Intro\n- one\n  - nested\n1. first\n- [ ] task\n-";
        assert_eq!(
            blocks(src),
            [
                Block::Paragraph,
                Block::Bullet(0),
                Block::Bullet(1),
                Block::Paragraph,
                Block::Bullet(0),
                Block::Paragraph
            ],
            "a lone `-` isn't one yet"
        );
        let on_item = focused(src, src.find("one").unwrap());
        let styled = spans(src, 16.0, &P, shown_lines(&on_item));
        assert_eq!(hidden(src, &styled), ["- ", "  - ", "- "], "markers and their indentation give way to bullets");
        assert_eq!(marks(src, &styled), ["1.", "[ ]", "-"], "numbers, task boxes and a lone dash stay");
        assert_eq!(
            bullet_marker(src, src.find("nested").unwrap()),
            Some(src.find("  - ").unwrap()..src.find("nested").unwrap())
        );
        assert_eq!(bullet_marker(src, 0), None);
    }

    #[test]
    fn a_selection_shows_every_line_it_touches_and_a_blurred_field_none() {
        let src = "# One\n## Two\n### Three";
        let mut edit = focused(src, 2);
        edit.set_selection(2, src.find("Two").unwrap());
        assert_eq!(hidden(src, &spans(src, 16.0, &P, shown_lines(&edit))), ["### "]);
        edit.focused = false;
        assert_eq!(hidden(src, &spans(src, 16.0, &P, shown_lines(&edit))), ["# ", "## ", "### "]);
    }
}

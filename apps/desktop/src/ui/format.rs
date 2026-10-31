//! Words for times and counts.

const MIN: i64 = 60_000;
const HOUR: i64 = 60 * MIN;
const DAY: i64 = 24 * HOUR;

/// "in 10 minutes", "in 3 hours", "tomorrow", "in 4 days", relative to `now`.
pub fn until(due: i64, now: i64) -> String {
    let d = (due - now).max(0);
    let plural = |n: i64, unit: &str| if n == 1 { format!("in 1 {unit}") } else { format!("in {n} {unit}s") };
    if d < MIN {
        "in under a minute".into()
    } else if d < HOUR {
        plural((d + MIN / 2) / MIN, "minute")
    } else if d < DAY - HOUR / 2 {
        plural((d + HOUR / 2) / HOUR, "hour")
    } else if d < 60 * DAY {
        match (d + DAY / 2) / DAY {
            1 => "tomorrow".into(),
            n => plural(n, "day"),
        }
    } else {
        plural((d + 15 * DAY) / (30 * DAY), "month")
    }
}

/// How far off a card is, for a tile's footer: "12 min", "3 hours", "tomorrow", "2 days", "3 months".
pub fn until_short(due: i64, now: i64) -> String {
    let d = (due - now).max(0);
    let plural = |n: i64, unit: &str| if n == 1 { format!("1 {unit}") } else { format!("{n} {unit}s") };
    if d < HOUR {
        format!("{} min", ((d + MIN / 2) / MIN).max(1))
    } else if d < DAY - HOUR / 2 {
        plural((d + HOUR / 2) / HOUR, "hour")
    } else if d < 60 * DAY {
        match (d + DAY / 2) / DAY {
            1 => "tomorrow".into(),
            n => plural(n, "day"),
        }
    } else {
        plural((d + 15 * DAY) / (30 * DAY), "month")
    }
}

/// "1 hidden side", "2 hidden sides".
pub fn hidden_sides(n: usize) -> String {
    if n == 1 { "1 hidden side".into() } else { format!("{n} hidden sides") }
}

/// "1 review", "5 reviews".
pub fn reviews(n: u32) -> String {
    if n == 1 { "1 review".into() } else { format!("{n} reviews") }
}

/// "1 lapse", "2 lapses".
pub fn lapses(n: u32) -> String {
    if n == 1 { "1 lapse".into() } else { format!("{n} lapses") }
}

/// Markdown as plain text on one line, for compact tiles: emphasis, code and heading marks dropped.
pub fn plain(markdown: &str) -> String {
    let words: Vec<&str> = markdown.split_whitespace().collect();
    let line = words.join(" ");
    line.trim_start_matches(['#', '>', '-', ' ']).replace("**", "").replace(['*', '`'], "")
}

/// "just now", "5 min ago", "2 h ago", "3 d ago".
pub fn ago(at: i64, now: i64) -> String {
    let d = (now - at).max(0);
    if d < MIN {
        "just now".into()
    } else if d < HOUR {
        format!("{} min ago", d / MIN)
    } else if d < DAY {
        format!("{} h ago", d / HOUR)
    } else {
        format!("{} d ago", d / DAY)
    }
}

/// "3 due · 5 new", leaving out what's zero; "Nothing due" when both are.
pub fn counts(due: u64, new: u64) -> String {
    match (due, new) {
        (0, 0) => "Nothing due".into(),
        (d, 0) => format!("{d} due"),
        (0, n) => format!("{n} new"),
        (d, n) => format!("{d} due · {n} new"),
    }
}

/// The first line of a card's front, without Markdown's emphasis marks, for lists.
pub fn title(front: &str) -> String {
    let line = front.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    let line = line.trim_start_matches(['#', '>', '-', '*', ' ']);
    line.replace("**", "").replace('`', "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn until_reads_naturally() {
        assert_eq!(until(10 * MIN, 0), "in 10 minutes");
        assert_eq!(until(HOUR, 0), "in 1 hour");
        assert_eq!(until(30 * HOUR, 0), "tomorrow");
        assert_eq!(until(4 * DAY, 0), "in 4 days");
        assert_eq!(until(2 * DAY - 5, 0), "in 2 days");
        assert_eq!(until(DAY - 5, 0), "tomorrow");
    }

    #[test]
    fn short_untils_fit_a_footer() {
        assert_eq!(until_short(10 * MIN, 0), "10 min");
        assert_eq!(until_short(3 * HOUR, 0), "3 hours");
        assert_eq!(until_short(30 * HOUR, 0), "tomorrow");
        assert_eq!(until_short(2 * DAY, 0), "2 days");
    }

    #[test]
    fn plain_drops_markup() {
        assert_eq!(plain("When do you use *ser*\nrather than `estar`?"), "When do you use ser rather than estar?");
    }

    #[test]
    fn titles_drop_markup() {
        assert_eq!(title("\n**la biblioteca**\nmore"), "la biblioteca");
        assert_eq!(title("# What does `?` do"), "What does ? do");
    }
}

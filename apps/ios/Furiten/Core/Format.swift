import Foundation

// The desktop's wording (apps/desktop/src/ui/format.rs, apps/web/src/format.ts), so every app says the same thing.

private let minute: Int64 = 60_000
private let hour = 60 * minute
private let day = 24 * hour

private func plural(_ n: Int64, _ one: String, _ many: String? = nil) -> String {
    "\(n) \(n == 1 ? one : many ?? one + "s")"
}

private func round(_ ms: Int64, _ unit: Int64) -> Int64 { (ms + unit / 2) / unit }

/// Hours up to 23½, then days (1 is "tomorrow") up to 60, then months.
private func distance(_ ms: Int64) -> (n: Int64, unit: String)? {
    if ms < hour { return (round(ms, minute), "minute") }
    if ms < day - hour / 2 { return (round(ms, hour), "hour") }
    if ms < 60 * day {
        let days = round(ms, day)
        return days == 1 ? nil : (days, "day")
    }
    return ((ms + 15 * day) / (30 * day), "month")
}

/// "in 10 minutes", "in 3 hours", "tomorrow", "in 4 days", "in 2 months".
func until(_ at: Int64, now: Int64 = nowMs()) -> String {
    let ms = max(0, at - now)
    if ms < minute { return "in under a minute" }
    guard let d = distance(ms) else { return "tomorrow" }
    return "in \(plural(d.n, d.unit))"
}

/// "12 min", "3 hours", "tomorrow", "2 days", "3 months".
func untilShort(_ at: Int64, now: Int64 = nowMs()) -> String {
    let ms = max(0, at - now)
    if ms < hour { return "\(max(1, round(ms, minute))) min" }
    guard let d = distance(ms) else { return "tomorrow" }
    return plural(d.n, d.unit)
}

func ago(_ at: Date, now: Date = .now) -> String {
    let ms = Int64(max(0, now.timeIntervalSince(at)) * 1000)
    if ms < minute { return "just now" }
    if ms < hour { return "\(ms / minute) min ago" }
    if ms < day { return "\(ms / hour) h ago" }
    return "\(ms / day) d ago"
}

func counts(_ due: Int, _ new: Int) -> String {
    if due == 0 && new == 0 { return "Nothing due" }
    if new == 0 { return "\(due) due" }
    if due == 0 { return "\(new) new" }
    return "\(due) due · \(new) new"
}

func hiddenSides(_ n: Int) -> String { plural(Int64(n), "hidden side") }
func reviews(_ n: Int) -> String { plural(Int64(n), "review") }
func lapses(_ n: Int) -> String { plural(Int64(n), "lapse") }

/// Markdown flattened to one line of plain text, for compact items and sizing tiles.
func plain(_ markdown: String) -> String {
    markdown
        .replacing(/```[^\n]*\n?/, with: "")
        .replacing(/!\[([^\]]*)\]\([^)]*\)/, with: { String($0.1) })
        .replacing(/\[([^\]]*)\]\([^)]*\)/, with: { String($0.1) })
        .replacing(/^[ \t]{0,3}(?:\#{1,6}\s+|>\s?|[-*+]\s+(?:\[[ xX]\]\s+)?|\d+[.)]\s+)/.anchorsMatchLineEndings(), with: "")
        .replacing(/\*\*|__|\*|_|~~|`/, with: "")
        .replacing(/\s+/, with: " ")
        .trimmingCharacters(in: .whitespacesAndNewlines)
}

/// How big a tile's front is, by its plain length (apps/desktop/src/ui/cards.rs).
enum FrontSize {
    case xl, l, m, s

    init(_ text: String) {
        let n = plain(text).count
        self = n <= 24 ? .xl : n <= 60 ? .l : n <= 140 ? .m : .s
    }
}

import Foundation

// The card editor's side editing (apps/desktop/src/editor.rs, apps/web/src/sides.ts): a card is its sides joined by
// `---` lines, and the editor gives each side a field of its own.

private func isSeparator(_ line: Substring) -> Bool {
    line.trimmingCharacters(in: .whitespaces) == "---"
}

/// Splits content into sides at `---` lines.
func splitSides(_ content: String) -> [String] {
    var sides: [String] = []
    var current: [Substring] = []
    for line in content.split(separator: "\n", omittingEmptySubsequences: false) {
        if isSeparator(line) {
            sides.append(current.joined(separator: "\n"))
            current = []
        } else {
            current.append(line)
        }
    }
    sides.append(current.joined(separator: "\n"))
    return sides
}

/// What's saved: the non-empty sides joined by `---` lines.
func joinSides(_ sides: [String]) -> String {
    sides
        .map { $0.replacing(/^\n+/, with: "").replacing(/\s+$/, with: "") }
        .filter { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
        .joined(separator: "\n---\n")
}

/// A side as typed or pasted into: when it has a finished `---` line (Return after it, or pasted), the sides it
/// splits into; otherwise nil. A `---` still on the last line is left alone, as it may be the start of something else.
func splitTyped(_ text: String) -> [String]? {
    let lines = text.split(separator: "\n", omittingEmptySubsequences: false)
    guard lines.dropLast().contains(where: isSeparator) else { return nil }
    return splitSides(text)
}

/// Tags as the server stores them: lowercase, trimmed, `#` stripped.
func normalizeTag(_ tag: String) -> String {
    tag.trimmingCharacters(in: .whitespaces).replacing(/^#+/, with: "").trimmingCharacters(in: .whitespaces).lowercased()
}

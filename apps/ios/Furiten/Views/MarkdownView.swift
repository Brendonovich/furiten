import SwiftUI

/// Renders one side of a card: headings, lists, quotes and code fences by block, inline Markdown within each.
struct MarkdownView: View {
    let text: String
    var selectable = false
    @Environment(\.palette) private var palette

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                switch block {
                case .heading(let level, let text):
                    inline(text).font(level == 1 ? .title.bold() : level == 2 ? .title2.bold() : .headline)
                case .code(let code):
                    ScrollView(.horizontal, showsIndicators: false) {
                        Text(code).font(.callout.monospaced()).padding(12)
                    }
                    .background(palette.inset, in: .rect(cornerRadius: 8))
                case .quote(let text):
                    inline(text)
                        .foregroundStyle(.secondary)
                        .padding(.leading, 12)
                        .overlay(alignment: .leading) { Rectangle().fill(palette.rule).frame(width: 3) }
                case .paragraph(let text):
                    inline(text)
                }
            }
        }
        .modifier(Selectable(on: selectable))
        // Links take the theme's accent.
        .tint(palette.accent)
    }

    @ViewBuilder
    private func inline(_ markdown: String) -> some View {
        let runs = Self.inlineRuns(markdown)
        let text = concatenated(runs)
        if runs.contains(where: isCode) {
            text.textRenderer(CodeChipRenderer(fill: palette.inset))
        } else {
            text
        }
    }

    /// Joins the runs into one Text, so a block still flows and wraps as a single paragraph. Only the code runs are
    /// split out — to carry their `InlineCode` tag and read in monospace; text runs keep their full Markdown styling
    /// (bold, italics, links).
    private func concatenated(_ runs: [InlineRun]) -> Text {
        var text = Text(verbatim: "")
        var first = true
        for run in runs {
            let part: Text
            switch run {
            case .text(let markdown):
                part = attributed(markdown)
            case .code(let code):
                // A narrow no-break space either side, in the surrounding face, keeps the chip's neighbours clear
                // without giving the line a place to break inside the span. Only the code itself is tagged and mono,
                // so a wrap can never leave an empty chip behind.
                part = Text(verbatim: "\u{202F}")
                    + Text(verbatim: code).fontDesign(.monospaced).customAttribute(InlineCode())
                    + Text(verbatim: "\u{202F}")
            }
            text = first ? part : text + part
            first = false
        }
        return text
    }

    private func isCode(_ run: InlineRun) -> Bool {
        if case .code = run { return true }
        return false
    }

    private func attributed(_ markdown: String) -> Text {
        let options = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        return Text((try? AttributedString(markdown: markdown, options: options)) ?? AttributedString(markdown))
    }

    private enum Block {
        case heading(Int, String)
        case code(String)
        case quote(String)
        case paragraph(String)
    }

    private var blocks: [Block] {
        var blocks: [Block] = []
        var paragraph: [String] = []
        var code: [String]?
        func flush() {
            let text = paragraph.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
            if !text.isEmpty {
                let lines = text.split(separator: "\n", omittingEmptySubsequences: false)
                if lines.allSatisfy({ $0.hasPrefix(">") }) {
                    blocks.append(.quote(lines.map { $0.dropFirst().trimmingCharacters(in: .whitespaces) }
                        .joined(separator: "\n")))
                } else {
                    blocks.append(.paragraph(lines.map(Self.bullet).joined(separator: "\n")))
                }
            }
            paragraph = []
        }
        for line in text.components(separatedBy: "\n") {
            if line.hasPrefix("```") {
                if let lines = code {
                    blocks.append(.code(lines.joined(separator: "\n")))
                    code = nil
                } else {
                    flush()
                    code = []
                }
            } else if code != nil {
                code?.append(line)
            } else if let heading = Self.heading(line) {
                flush()
                blocks.append(heading)
            } else if line.trimmingCharacters(in: .whitespaces).isEmpty {
                flush()
            } else {
                paragraph.append(line)
            }
        }
        if let lines = code { blocks.append(.code(lines.joined(separator: "\n"))) }
        flush()
        return blocks
    }

    private static func heading(_ line: String) -> Block? {
        let level = line.prefix { $0 == "#" }.count
        guard (1...6).contains(level), line.dropFirst(level).first == " " else { return nil }
        return .heading(level, String(line.dropFirst(level + 1)))
    }

    /// `- item` and `* item` read better as bullets.
    private static func bullet(_ line: Substring) -> String {
        let indent = line.prefix { $0 == " " }
        let rest = line.dropFirst(indent.count)
        if rest.hasPrefix("- ") || rest.hasPrefix("* ") {
            return indent + "• " + rest.dropFirst(2)
        }
        return String(line)
    }

    /// Inline Markdown split at code spans, so each code span can draw as a chip.
    private enum InlineRun {
        case text(String)
        case code(String)
    }

    /// Splits inline Markdown into text and code runs at matching backtick pairs. A code span opens and closes with
    /// the same run of backticks (so `` ` code with \` inside ` ``), and an unmatched backtick stays in its text run,
    /// rendered as it is now.
    private static func inlineRuns(_ markdown: String) -> [InlineRun] {
        var runs: [InlineRun] = []
        var start = markdown.startIndex
        var i = markdown.startIndex
        func backticks(at j: String.Index) -> Int {
            var n = 0
            var k = j
            while k < markdown.endIndex, markdown[k] == "`" { n += 1; k = markdown.index(after: k) }
            return n
        }
        while i < markdown.endIndex {
            if markdown[i] == "`" {
                let open = backticks(at: i)
                var j = markdown.index(i, offsetBy: open)
                var closed = false
                while j < markdown.endIndex {
                    if markdown[j] == "`" {
                        let close = backticks(at: j)
                        if close == open {
                            if start < i { runs.append(.text(String(markdown[start..<i]))) }
                            runs.append(.code(codeContent(String(markdown[markdown.index(i, offsetBy: open)..<j]))))
                            j = markdown.index(j, offsetBy: close)
                            i = j
                            start = j
                            closed = true
                            break
                        }
                        j = markdown.index(j, offsetBy: close)
                    } else {
                        j = markdown.index(after: j)
                    }
                }
                if closed { continue }
            }
            i = markdown.index(after: i)
        }
        if start < markdown.endIndex { runs.append(.text(String(markdown[start...]))) }
        return runs
    }

    /// A code span drops one of a pair of spaces it opens and closes with (CommonMark).
    private static func codeContent(_ raw: String) -> String {
        var s = raw
            .replacingOccurrences(of: "\r\n", with: " ")
            .replacingOccurrences(of: "\n", with: " ")
            .replacingOccurrences(of: "\r", with: " ")
        if s.first == " ", s.last == " ", s.count >= 2, !s.trimmingCharacters(in: .whitespaces).isEmpty {
            s.removeFirst()
            s.removeLast()
        }
        return s
    }
}

/// Marks a code run so the text renderer can draw its chip behind it.
private struct InlineCode: TextAttribute {}

/// Draws inline code as the web app and desktop do: a rounded tint behind the monospaced run, a little wider than it,
/// before the line's glyphs are drawn. A span can be split into several runs by the layout; consecutive tagged
/// fragments share one rect, and empty fragments add nothing, so a wrap can't leave a stray box on the line.
private struct CodeChipRenderer: TextRenderer {
    var fill: Color

    func draw(layout: Text.Layout, in ctx: inout GraphicsContext) {
        let outset: CGFloat = 3
        for line in layout {
            var span: CGRect?
            for run in line {
                guard run[InlineCode.self] != nil, run.typographicBounds.rect.width > 0 else {
                    if let span { fillChip(span, into: &ctx) }
                    span = nil
                    continue
                }
                let rect = run.typographicBounds.rect.insetBy(dx: -outset, dy: 0)
                span = span.map { $0.union(rect) } ?? rect
            }
            if let span { fillChip(span, into: &ctx) }
        }
        for line in layout { ctx.draw(line) }
    }

    private func fillChip(_ rect: CGRect, into ctx: inout GraphicsContext) {
        ctx.fill(Path(roundedRect: rect, cornerRadius: 4), with: .color(fill))
    }
}

/// Selecting text needs a long press, which would fight a tile's or review card's tap.
private struct Selectable: ViewModifier {
    let on: Bool

    func body(content: Content) -> some View {
        if on { content.textSelection(.enabled) } else { content }
    }
}

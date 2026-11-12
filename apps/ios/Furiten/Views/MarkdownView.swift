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

    private func inline(_ markdown: String) -> Text {
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
}

/// Selecting text needs a long press, which would fight a tile's or review card's tap.
private struct Selectable: ViewModifier {
    let on: Bool

    func body(content: Content) -> some View {
        if on { content.textSelection(.enabled) } else { content }
    }
}

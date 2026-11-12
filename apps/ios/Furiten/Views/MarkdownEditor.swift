import SwiftUI
import UIKit

/// One side's field: Markdown styled as it's typed (as `MarkdownView` would show it, with the syntax dimmed), so
/// there's no separate preview. Above the keyboard, a toolbar for common Markdown and a new side.
struct MarkdownEditor: UIViewRepresentable {
    @Environment(\.palette) private var palette
    @Binding var text: String
    var placeholder: String
    /// The text's size, in points at the default Dynamic Type size, which scales it. Review and a card's detail show
    /// every side at `.title2`, so that's what it's written at.
    var size: CGFloat = 22
    /// Take the keyboard as soon as the field can.
    var wantsFocus = false
    /// Where the caret goes when it does (UTF-16 offset); nil leaves it where UIKit puts it.
    var caret: Int?
    var onFocus: (Bool) -> Void = { _ in }
    var onAddSide: () -> Void = {}
    /// Backspace at the very start: join this side to the one before. Nil on the first side.
    var onJoin: (() -> Void)?
    /// An arrow key past the top (-1) or bottom (1) of the side, on a hardware keyboard; true when it moved.
    var onLeave: (Int) -> Bool = { _ in false }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeUIView(context: Context) -> TextView {
        let view = TextView()
        view.delegate = context.coordinator
        context.coordinator.view = view
        view.size = size
        view.backgroundColor = .clear
        // The caret, list markers and links (`UIColor.tintColor` below) take the theme's accent.
        view.accent = palette.accent
        view.tintColor = UIColor(palette.accent)
        view.isScrollEnabled = false
        view.textContainerInset = .zero
        view.textContainer.lineFragmentPadding = 0
        view.font = MarkdownStyle.body(size)
        view.autocapitalizationType = .sentences
        view.smartDashesType = .no
        view.smartQuotesType = .no
        view.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        view.inputAccessoryView = context.coordinator.toolbar()
        view.text = text
        context.coordinator.sent = text
        MarkdownStyle.apply(to: view)
        view.placeholder.font = MarkdownStyle.body(size)
        view.placeholder.text = placeholder
        view.placeholder.isHidden = !text.isEmpty
        return view
    }

    func updateUIView(_ view: TextView, context: Context) {
        context.coordinator.parent = self
        if view.accent != palette.accent {
            view.accent = palette.accent
            view.tintColor = UIColor(palette.accent)
            MarkdownStyle.apply(to: view)
        }
        if view.size != size {
            view.size = size
            view.placeholder.font = MarkdownStyle.body(size)
            MarkdownStyle.apply(to: view)
        }
        if view.text != text {
            view.text = text
            context.coordinator.sent = text
            MarkdownStyle.apply(to: view)
        }
        view.placeholder.text = placeholder
        view.placeholder.isHidden = !text.isEmpty
        view.wantsFocus = wantsFocus
        view.onJoin = onJoin
        view.onLeave = onLeave
        if wantsFocus { view.caret = caret }
        if wantsFocus && view.window != nil {
            DispatchQueue.main.async { view.focus() }
        }
    }

    func sizeThatFits(_ proposal: ProposedViewSize, uiView: TextView, context: Context) -> CGSize? {
        guard let width = proposal.width, width.isFinite else { return nil }
        let height = uiView.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude)).height
        return CGSize(width: width, height: max(height, MarkdownStyle.body(size).lineHeight))
    }

    final class TextView: UITextView {
        let placeholder = UILabel()
        var size: CGFloat = 22
        var accent: Color?
        var wantsFocus = false
        var caret: Int?
        var onJoin: (() -> Void)?
        var onLeave: (Int) -> Bool = { _ in false }

        override init(frame: CGRect, textContainer: NSTextContainer?) {
            super.init(frame: frame, textContainer: textContainer)
            placeholder.textColor = .placeholderText
            placeholder.translatesAutoresizingMaskIntoConstraints = false
            addSubview(placeholder)
            NSLayoutConstraint.activate([
                placeholder.leadingAnchor.constraint(equalTo: leadingAnchor),
                placeholder.topAnchor.constraint(equalTo: topAnchor),
            ])
        }

        required init?(coder: NSCoder) { fatalError() }

        override func didMoveToWindow() {
            super.didMoveToWindow()
            if wantsFocus && window != nil { DispatchQueue.main.async { self.focus() } }
        }

        /// Takes the keyboard, with the caret where it was asked to go.
        func focus() {
            if !isFirstResponder { becomeFirstResponder() }
            if let caret {
                selectedRange = NSRange(location: min(caret, (text as NSString).length), length: 0)
                self.caret = nil
            }
        }

        override func deleteBackward() {
            if selectedRange == NSRange(location: 0, length: 0), let onJoin { return onJoin() }
            super.deleteBackward()
        }

        override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
            if let key = presses.first?.key, key.modifierFlags.intersection([.shift, .command, .alternate, .control]).isEmpty,
               selectedRange.length == 0 {
                let up = key.keyCode == .keyboardUpArrow && selectedRange.location == 0
                let down = key.keyCode == .keyboardDownArrow && selectedRange.location == (text as NSString).length
                if (up && onLeave(-1)) || (down && onLeave(1)) { return }
            }
            super.pressesBegan(presses, with: event)
        }
    }

    @MainActor
    final class Coordinator: NSObject, UITextViewDelegate {
        var parent: MarkdownEditor
        /// The text last given to the binding, so an edit reaches it once.
        var sent = ""
        weak var view: UITextView?

        init(_ parent: MarkdownEditor) { self.parent = parent }

        func textViewDidBeginEditing(_ textView: UITextView) {
            (textView as? TextView)?.wantsFocus = false
            parent.onFocus(true)
        }

        func textViewDidEndEditing(_ textView: UITextView) { parent.onFocus(false) }

        func textViewDidChange(_ textView: UITextView) {
            (textView as? TextView)?.placeholder.isHidden = !textView.text.isEmpty
            if textView.markedTextRange == nil { MarkdownStyle.apply(to: textView) }
            guard textView.text != sent else { return }
            sent = textView.text
            parent.text = textView.text
        }

        /// Return in a list item continues the list, or ends it on an empty item.
        func textView(_ textView: UITextView, shouldChangeTextIn range: NSRange, replacementText text: String) -> Bool {
            guard text == "\n", range.length == 0 else { return true }
            let ns = textView.text as NSString
            let line = ns.lineRange(for: NSRange(location: range.location, length: 0))
            let before = ns.substring(with: NSRange(location: line.location, length: range.location - line.location))
            guard let match = before.firstMatch(of: #/^(\s*)([-*+]|\d+\.)( \[[ xX]\])? /#) else { return true }
            if match.output.0.count == before.count {
                replace(NSRange(location: line.location, length: before.utf16.count), with: "", in: textView)
                return false
            }
            var marker = String(match.output.2)
            if let n = Int(marker.dropLast()) { marker = "\(n + 1)." }
            let check = match.output.3 == nil ? "" : " [ ]"
            replace(range, with: "\n\(match.output.1)\(marker)\(check) ", in: textView)
            return false
        }

        /// The toolbar, with a gap under it so it doesn't sit on the keyboard.
        func toolbar() -> UIView {
            let gap: CGFloat = 8
            let container = UIView(frame: CGRect(x: 0, y: 0, width: 400, height: 44 + gap))
            container.backgroundColor = .clear
            let bar = UIToolbar(frame: CGRect(x: 0, y: 0, width: 400, height: 44))
            bar.autoresizingMask = [.flexibleWidth]
            container.addSubview(bar)
            let appearance = UIToolbarAppearance()
            appearance.configureWithTransparentBackground()
            bar.standardAppearance = appearance
            bar.compactAppearance = appearance
            func item(_ title: String, _ image: String, _ action: @escaping (UITextView) -> Void) -> UIBarButtonItem {
                UIBarButtonItem(title: title, image: UIImage(systemName: image), primaryAction: UIAction(title: title) { [weak self] _ in
                    guard let view = self?.view else { return }
                    action(view)
                })
            }
            bar.items = [
                item("Bold", "bold") { [unowned self] in wrap("**", in: $0) },
                item("Italic", "italic") { [unowned self] in wrap("*", in: $0) },
                item("Code", "chevron.left.forwardslash.chevron.right") { [unowned self] in code(in: $0) },
                item("Heading", "number") { [unowned self] in heading(in: $0) },
                item("List", "list.bullet") { [unowned self] in prefixLines("- ", in: $0) },
                .flexibleSpace(),
                UIBarButtonItem(title: "Add side", image: UIImage(systemName: "plus.rectangle.on.rectangle"),
                                primaryAction: UIAction(title: "Add side") { [weak self] _ in self?.parent.onAddSide() }),
                UIBarButtonItem(title: "Hide keyboard", image: UIImage(systemName: "keyboard.chevron.compact.down"),
                                primaryAction: UIAction(title: "Hide keyboard") { [weak self] _ in self?.view?.resignFirstResponder() }),
            ]
            return container
        }

        /// Wraps the selection in a marker (or an empty pair around the cursor), or unwraps it if it's wrapped already.
        private func wrap(_ marker: String, in view: UITextView) {
            let ns = view.text as NSString
            let r = view.selectedRange
            let m = marker.utf16.count
            if r.location >= m, NSMaxRange(r) + m <= ns.length,
               ns.substring(with: NSRange(location: r.location - m, length: m)) == marker,
               ns.substring(with: NSRange(location: NSMaxRange(r), length: m)) == marker {
                replace(NSRange(location: r.location - m, length: r.length + 2 * m), with: ns.substring(with: r), in: view,
                        select: NSRange(location: r.location - m, length: r.length))
                return
            }
            replace(r, with: marker + ns.substring(with: r) + marker, in: view, select: NSRange(location: r.location + m, length: r.length))
        }

        /// Inline code, or a fenced block around lines.
        private func code(in view: UITextView) {
            let ns = view.text as NSString
            let r = view.selectedRange
            let selected = ns.substring(with: r)
            guard selected.contains("\n") else { return wrap("`", in: view) }
            replace(r, with: "```\n\(selected)\n```", in: view, select: NSRange(location: r.location + 4, length: r.length))
        }

        /// `#` → `##` → `###` → none, on the cursor's line.
        private func heading(in view: UITextView) {
            let ns = view.text as NSString
            let r = view.selectedRange
            let line = ns.lineRange(for: NSRange(location: r.location, length: 0))
            let text = ns.substring(with: line)
            let level = text.prefix { $0 == "#" }.count
            let current = level > 0 && text.dropFirst(level).first == " " ? level + 1 : 0
            let next = current == 0 ? "# " : level >= 3 ? "" : String(repeating: "#", count: level + 1) + " "
            let delta = next.utf16.count - current
            replace(NSRange(location: line.location, length: current), with: next, in: view,
                    select: NSRange(location: max(line.location, r.location + delta), length: r.length))
        }

        /// Toggles a prefix on every selected line: on all of them, unless they all have it.
        private func prefixLines(_ prefix: String, in view: UITextView) {
            let ns = view.text as NSString
            let r = view.selectedRange
            var lines = ns.lineRange(for: r)
            if lines.length > 0, ns.character(at: NSMaxRange(lines) - 1) == 10 { lines.length -= 1 }
            let parts = ns.substring(with: lines).components(separatedBy: "\n")
            let off = parts.allSatisfy { $0.hasPrefix(prefix) }
            let changed = parts.map { off ? String($0.dropFirst(prefix.count)) : prefix + $0 }.joined(separator: "\n")
            let select = r.length == 0
                ? NSRange(location: max(lines.location, r.location + (off ? -1 : 1) * prefix.utf16.count), length: 0)
                : NSRange(location: lines.location, length: changed.utf16.count)
            replace(lines, with: changed, in: view, select: select)
        }

        /// Edits through the text input system, so they can be undone.
        private func replace(_ range: NSRange, with string: String, in view: UITextView, select: NSRange? = nil) {
            guard let start = view.position(from: view.beginningOfDocument, offset: range.location),
                  let end = view.position(from: start, offset: range.length),
                  let textRange = view.textRange(from: start, to: end) else { return }
            view.replace(textRange, withText: string)
            if let select { view.selectedRange = select }
            textViewDidChange(view)
        }
    }
}

/// Live styling for Markdown being written: the look of `MarkdownView`, syntax kept but dimmed.
enum MarkdownStyle {
    /// Text at `size` points, scaled for Dynamic Type as `.title2` is (how review shows a side).
    static func body(_ size: CGFloat) -> UIFont {
        UIFontMetrics(forTextStyle: .title2).scaledFont(for: .systemFont(ofSize: size))
    }

    @MainActor
    static func apply(to view: UITextView) {
        let size = (view as? MarkdownEditor.TextView)?.size ?? 22
        let body = body(size)
        let mono = mono(size)
        let selected = view.selectedRange
        let storage = view.textStorage
        let ns = storage.string as NSString
        storage.beginEditing()
        let all = NSRange(location: 0, length: ns.length)
        let base: [NSAttributedString.Key: Any] = [.font: body, .foregroundColor: UIColor.label, .paragraphStyle: paragraph]
        // Only the attributes styling sets are reset. The paragraph style is added where it's missing and otherwise left
        // alone: replacing it re-lays out the paragraph mid-edit, and UIKit moved the caret to the line's end on delete.
        for key in [NSAttributedString.Key.backgroundColor, .strikethroughStyle] { storage.removeAttribute(key, range: all) }
        storage.addAttributes([.font: body, .foregroundColor: UIColor.label], range: all)
        storage.enumerateAttribute(.paragraphStyle, in: all) { value, range, _ in
            if (value as? NSParagraphStyle) != paragraph { storage.addAttribute(.paragraphStyle, value: paragraph, range: range) }
        }
        var fenced = false
        ns.enumerateSubstrings(in: NSRange(location: 0, length: ns.length), options: .byLines) { line, range, _, _ in
            guard let line else { return }
            if line.hasPrefix("```") {
                fenced.toggle()
                storage.addAttributes([.font: mono, .foregroundColor: UIColor.tertiaryLabel], range: range)
                return
            }
            if fenced {
                storage.addAttributes([.font: mono, .backgroundColor: UIColor.tertiarySystemFill], range: range)
                return
            }
            style(line: line, at: range, size: size, in: storage)
        }
        storage.endEditing()
        view.typingAttributes = base
        view.selectedRange = selected
    }

    /// The line spacing review and a card's detail show a side with (`.lineSpacing(4)`).
    @MainActor private static let paragraph: NSParagraphStyle = {
        let style = NSMutableParagraphStyle()
        style.lineSpacing = 4
        return style
    }()

    /// Code, a little smaller than the text around it (`.cm-md-code` on the web).
    private static func mono(_ size: CGFloat) -> UIFont {
        UIFontMetrics(forTextStyle: .title2).scaledFont(for: .monospacedSystemFont(ofSize: (size * 0.88).rounded(), weight: .regular))
    }
    private static let marker = UIColor.tertiaryLabel

    private static func style(line: String, at range: NSRange, size: CGFloat, in storage: NSTextStorage) {
        func sub(_ r: Range<String.Index>) -> NSRange {
            let n = NSRange(r, in: line)
            return NSRange(location: range.location + n.location, length: n.length)
        }
        if let m = line.firstMatch(of: #/^(#{1,6}) /#) {
            let level = m.output.1.count
            // In proportion to the side's text, so a heading is always the larger.
            let font = bold(level == 1 ? size * 1.3 : level == 2 ? size * 1.15 : size)
            storage.addAttribute(.font, value: font, range: range)
            storage.addAttribute(.foregroundColor, value: marker, range: sub(m.range))
        } else if let m = line.firstMatch(of: #/^>\s?/#) {
            storage.addAttribute(.foregroundColor, value: UIColor.secondaryLabel, range: range)
            storage.addAttribute(.foregroundColor, value: marker, range: sub(m.range))
        } else if let m = line.firstMatch(of: #/^\s*(?:[-*+]|\d+\.) /#) {
            storage.addAttribute(.foregroundColor, value: UIColor.tintColor, range: sub(m.range))
        }
        func spans(_ regex: NSRegularExpression) -> [NSTextCheckingResult] {
            regex.matches(in: line, range: NSRange(location: 0, length: (line as NSString).length))
        }
        func shift(_ r: NSRange) -> NSRange { NSRange(location: range.location + r.location, length: r.length) }
        for m in spans(strong) {
            traits(.traitBold, shift(m.range), storage)
            markers(shift(m.range), 2, storage)
        }
        for m in spans(emphasis) {
            traits(.traitItalic, shift(m.range), storage)
            markers(shift(m.range), 1, storage)
        }
        for m in spans(strike) {
            storage.addAttribute(.strikethroughStyle, value: NSUnderlineStyle.single.rawValue, range: shift(m.range))
            markers(shift(m.range), 2, storage)
        }
        for m in spans(link) {
            storage.addAttribute(.foregroundColor, value: marker, range: shift(m.range))
            storage.addAttribute(.foregroundColor, value: UIColor.tintColor, range: shift(m.range(at: 1)))
        }
        for m in spans(code) {
            storage.addAttributes([.font: mono(size), .backgroundColor: UIColor.tertiarySystemFill], range: shift(m.range))
            markers(shift(m.range), 1, storage)
        }
    }

    // Swift's Regex has no lookbehind, which emphasis needs.
    private static let strong = try! NSRegularExpression(pattern: #"(\*\*|__)(?=\S)(.+?)(?<=\S)\1"#)
    private static let emphasis = try! NSRegularExpression(pattern: #"(?<![*\w])([*_])(?=[^\s*_])(.+?)(?<=[^\s*_])\1(?![*\w])"#)
    private static let strike = try! NSRegularExpression(pattern: #"~~(?=\S)(.+?)(?<=\S)~~"#)
    private static let link = try! NSRegularExpression(pattern: #"\[([^\]]+)\]\(([^)\s]+)\)"#)
    private static let code = try! NSRegularExpression(pattern: #"`[^`]+`"#)

    private static func bold(_ size: CGFloat) -> UIFont {
        UIFontMetrics(forTextStyle: .title2).scaledFont(for: .systemFont(ofSize: size.rounded(), weight: .bold))
    }

    /// Adds a trait to whatever font each part of the range already has, so bold in a heading stays heading-sized.
    private static func traits(_ trait: UIFontDescriptor.SymbolicTraits, _ range: NSRange, _ storage: NSTextStorage) {
        storage.enumerateAttribute(.font, in: range) { value, r, _ in
            guard let font = value as? UIFont,
                  let descriptor = font.fontDescriptor.withSymbolicTraits(font.fontDescriptor.symbolicTraits.union(trait)) else { return }
            storage.addAttribute(.font, value: UIFont(descriptor: descriptor, size: 0), range: r)
        }
    }

    /// Dims the `count` characters at each end of a span.
    private static func markers(_ range: NSRange, _ count: Int, _ storage: NSTextStorage) {
        storage.addAttribute(.foregroundColor, value: marker, range: NSRange(location: range.location, length: count))
        storage.addAttribute(.foregroundColor, value: marker, range: NSRange(location: NSMaxRange(range) - count, length: count))
    }
}

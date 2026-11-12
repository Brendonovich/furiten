import SwiftUI

/// The card editor (apps/web/src/Editor.tsx): a field for each side, styled as it's written. A line of only `---`
/// then Return splits a side, Backspace at the start of a side joins it to the one before, pasting splits on `---`,
/// and on a hardware keyboard the arrows move between sides and ⌘Return saves. Tags are chips with suggestions from
/// the library.
struct CardEditor: View {
    @Environment(Library.self) private var library
    @Environment(\.palette) private var palette
    @Environment(\.dismiss) private var dismiss
    let editing: Editing

    @State private var sides: [String]
    @State private var tags: [String]
    @State private var deck: String
    @State private var draft = ""
    @State private var saving = false
    @State private var error: String?
    @State private var confirmDelete = false
    @State private var libraryTags: [TagSummary] = []
    /// The side that should take the keyboard next, and where its caret goes.
    @State private var pending: Int?
    @State private var caret: Int?
    @FocusState private var tagsFocused: Bool
    /// How tall the sheet is. A new card opens at full height, since the keyboard it opens with needs it anyway: at
    /// half it would rise, wait for the keyboard, then grow. Back to half when the keyboard goes.
    @State private var detent: PresentationDetent

    init(editing: Editing) {
        self.editing = editing
        switch editing {
        case .new(let deck):
            _sides = State(initialValue: [""])
            _tags = State(initialValue: [])
            _deck = State(initialValue: deck)
            _pending = State(initialValue: 0)
            _detent = State(initialValue: .large)
        case .edit(let card):
            _sides = State(initialValue: splitSides(card.content))
            _tags = State(initialValue: card.tags)
            _deck = State(initialValue: card.deckId)
            _detent = State(initialValue: .medium)
        }
    }

    private var initial: CardView? {
        if case .edit(let card) = editing { card } else { nil }
    }

    private var dirty: Bool {
        guard let initial else { return !joinSides(sides).isEmpty || !tags.isEmpty }
        return joinSides(sides) != joinSides(splitSides(initial.content)) || tags != initial.tags || deck != initial.deckId
    }

    var body: some View {
        NavigationStack {
            // Everything hangs from the top: the card (at least as tall as half the sheet's room, and growing with what's
            // written), Add side under its last side, then the deck and tags. Nothing is pinned to the sheet's bottom,
            // because a system sheet changing height (dragged, or for the keyboard) moves first and resizes its content
            // only at the end, and whatever sits at its bottom slides away and pops back; this way only empty space does.
            ScrollViewReader { scroll in
                ScrollView {
                    VStack(spacing: 10) {
                        card
                        meta
                    }
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .padding(.bottom, 8)
                }
                // While a tag is typed, its row stays in sight above the suggestions, however the pills wrap.
                .onChange(of: tagsFocused) { _, focused in
                    if focused { revealTags(scroll) }
                }
                .onChange(of: draft) { revealTags(scroll) }
                .onChange(of: tags) { if tagsFocused { revealTags(scroll) } }
            }
            .scrollDismissesKeyboard(.interactively)
            // The tag field's keyboard toolbar, as the sides have the Markdown one: suggestions, by the thumb.
            .safeAreaInset(edge: .bottom, spacing: 0) {
                if tagsFocused { suggestionBar }
            }
            // Under the keyboard too: the Markdown toolbar above it is transparent and shows what's behind.
            .background { palette.page.ignoresSafeArea() }
            .navigationTitle(initial == nil ? "New card" : "Edit card")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button(role: .cancel) { close() } }
                if initial != nil {
                    ToolbarItem(placement: .topBarTrailing) {
                        Menu("More", systemImage: "ellipsis") {
                            Button("Delete card", systemImage: "trash", role: .destructive) { confirmDelete = true }
                        }
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(role: .confirm) { Task { await save() } }
                        .disabled(saving)
                        .keyboardShortcut(.return, modifiers: .command)
                }
            }
        }
        // Half the screen to start; drag it up for room to write more. The keyboard brings it up and takes it back.
        .presentationDetents([.medium, .large], selection: $detent)
        // The sheet's own surface, opaque: the page covers it anyway, but not for the frame the sheet resizes in when
        // the keyboard goes, where the system's glass showed through, lit by what's behind the sheet.
        .presentationBackground(palette.page)
        .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardWillHideNotification)) { _ in
            detent = .medium
        }
        .confirmationDialog("Delete this card?", isPresented: $confirmDelete, titleVisibility: .visible) {
            Button("Delete card", role: .destructive) {
                guard let initial else { return }
                Task {
                    await library.write { try await $0.deleteCard(initial.id) }
                    close()
                }
            }
        }
        .task(id: deck) {
            // Suggestions come from the deck the card is going into, then the rest of the library.
            libraryTags = (try? await library.core?.tags(deck: deck)) ?? []
        }
        .interactiveDismissDisabled(dirty)
        #if DEBUG
        .task {
            guard let text = Scripted.tagging else { return }
            try? await Task.sleep(for: .seconds(1))
            pending = nil
            tagsFocused = true
            draft = text
        }
        .task {
            guard Scripted.unfocus > 0 else { return }
            try? await Task.sleep(for: .seconds(Scripted.unfocus))
            UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
        }
        #endif
    }

    private func close() { dismiss() }

    /// Puts the keyboard in side `i`, with the caret at `at` (UTF-16), or where UIKit puts it.
    private func focus(_ i: Int, at: Int? = nil) {
        caret = at
        pending = i
    }

    /// Side `i` onto the end of the one before, on a new line, with the caret where they meet.
    private func join(_ i: Int) {
        guard i > 0, i < sides.count else { return }
        let prev = sides[i - 1]
        let value = sides[i]
        let gap = !prev.isEmpty && !value.isEmpty
        sides.replaceSubrange(i - 1...i, with: [gap ? "\(prev)\n\(value)" : prev + value])
        focus(i - 1, at: prev.utf16.count + (gap ? 1 : 0))
    }

    /// An arrow key past the top or bottom of side `i`: into the end of the one above or the start of the one below.
    private func leave(_ i: Int, by step: Int) -> Bool {
        let to = i + step
        guard sides.indices.contains(to) else { return false }
        focus(to, at: step < 0 ? sides[to].utf16.count : 0)
        return true
    }

    private func side(_ i: Int) -> Binding<String> {
        Binding {
            i < sides.count ? sides[i] : ""
        } set: { value in
            guard i < sides.count else { return }
            if let parts = splitTyped(value) {
                sides.replaceSubrange(i...i, with: parts)
                focus(i + parts.count - 1)
            } else {
                sides[i] = value
            }
        }
    }

    /// A new side after the given one (the keyboard toolbar's), or at the end; an empty side there already is used.
    private func addSide(after i: Int?) {
        let at = i ?? sides.count - 1
        if sides[at].trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            focus(at)
        } else if at + 1 < sides.count, sides[at + 1].trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            focus(at + 1)
        } else {
            sides.insert("", at: at + 1)
            focus(at + 1)
        }
    }

    /// The sides, divided by rules, with Add side at the card's foot.
    private var card: some View {
        VStack(alignment: .leading, spacing: 0) {
            sidesStack.padding(.horizontal, 24)
            Spacer(minLength: 0)
            HStack {
                Button("Side", systemImage: "plus") { addSide(after: nil) }
                    .font(.subheadline.weight(.medium))
                    .buttonStyle(PillButtonStyle())
                Spacer()
                Text("or type --- on its own line")
                    .font(.footnote)
                    .foregroundStyle(.tertiary)
            }
            // The pill sits as far in from the card's left edge as from its bottom, so its corner nests in the
            // card's corner. Its label then lines up with the sides' text.
            .padding(.leading, 12)
            .padding(.trailing, 24)
            .padding(.vertical, 12)
        }
        .frame(maxWidth: .infinity, minHeight: 300, alignment: .top)
        // A tap on the card's empty space writes at the end of the last side.
        .background {
            Color.clear.contentShape(.rect).onTapGesture { focus(sides.count - 1, at: sides[sides.count - 1].utf16.count) }
        }
        .cardSurface(radius: 18)
    }

    private var sidesStack: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(sides.indices, id: \.self) { i in
                if i > 0 { SideRule().padding(.horizontal, -24) }
                MarkdownEditor(
                    text: side(i),
                    placeholder: sides.count == 1 ? "Write in Markdown" : "Empty side",
                    wantsFocus: pending == i,
                    caret: pending == i ? caret : nil,
                    onFocus: { if $0 { pending = nil } },
                    onAddSide: { addSide(after: i) },
                    onJoin: i > 0 ? { join(i) } : nil,
                    onLeave: { leave(i, by: $0) }
                )
                // Review's spacing: the rule keeps 20 either side, and the card 26 above its first side.
                .padding(.top, i == 0 ? 26 : 0)
                .padding(.bottom, i == sides.count - 1 ? 20 : 0)
            }
        }
    }

    /// Any error, then one row of pills: the deck, the tags and a field for the next tag.
    private var meta: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let error { Text(error).font(.subheadline).foregroundStyle(.red) }
            FlowLayout(spacing: 6) {
                Menu {
                    Picker("Deck", selection: $deck) {
                        ForEach(deckTree(library.decks), id: \.deck.id) { row in
                            Text(deckPath(library.decks, row.deck.id)).tag(row.deck.id)
                        }
                    }
                } label: {
                    HStack(spacing: 5) {
                        Image(systemName: "book.closed").foregroundStyle(.secondary)
                        Text(deckPath(library.decks, deck)).lineLimit(1)
                        Image(systemName: "chevron.down").font(.caption2.weight(.semibold)).foregroundStyle(.secondary)
                    }
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.primary)
                    .padding(.horizontal, 12)
                    .frame(height: 32)
                    .background(.fill.tertiary, in: .capsule)
                }
                .tint(.primary)
                .accessibilityLabel("Deck, \(deckPath(library.decks, deck))")
                tagsField
            }
            .id(tagsRow)
        }
    }

    private let tagsRow = "tags"

    private func revealTags(_ scroll: ScrollViewProxy) {
        // After the keyboard and the suggestions have taken their room.
        DispatchQueue.main.async {
            withAnimation(.snappy(duration: 0.25)) { scroll.scrollTo(tagsRow, anchor: .bottom) }
        }
    }

    @ViewBuilder private var tagsField: some View {
        ForEach(tags, id: \.self) { tag in
            HStack(spacing: 2) {
                Text("#\(tag)")
                Button("Remove \(tag)", systemImage: "xmark") { tags.removeAll { $0 == tag } }
                    .labelStyle(.iconOnly)
                    .font(.caption2.weight(.bold))
                    .frame(width: 24, height: 24)
                    .buttonStyle(.plain)
                    .opacity(0.7)
            }
            .font(.subheadline.weight(.medium))
            .foregroundStyle(palette.accent)
            .padding(.leading, 12)
            .padding(.trailing, 4)
            .frame(height: 32)
            .background(palette.accent.quaternary, in: .capsule)
        }
        // A pill until it's tapped, then it widens to type in.
        HStack(spacing: 4) {
            Image(systemName: "plus").font(.footnote.weight(.medium)).foregroundStyle(.secondary)
            TextField("Tag", text: $draft)
                .font(.subheadline.weight(.medium))
                .accessibilityLabel("Add tag")
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .submitLabel(.done)
                .focused($tagsFocused)
                .onSubmit {
                    // Return takes the top suggestion and stays for the next tag; on an empty field it's done.
                    guard !draft.isEmpty else { return }
                    addTag(suggestions.first ?? draft)
                    tagsFocused = true
                }
                .onChange(of: draft) { _, value in
                    guard value.contains(where: { $0 == "," || $0.isWhitespace }) else { return }
                    let parts = value.split(omittingEmptySubsequences: false, whereSeparator: { $0 == "," || $0.isWhitespace })
                    for part in parts.dropLast() { addTag(String(part)) }
                    draft = String(parts.last ?? "")
                }
        }
        .padding(.leading, 9)
        .padding(.trailing, 10)
        .frame(width: tagsFocused || !draft.isEmpty ? 150 : 76, height: 32)
        .overlay { Capsule().strokeBorder(tagsFocused ? .tertiary : .quaternary, lineWidth: 1) }
        .contentShape(.capsule)
        .onTapGesture { tagsFocused = true }
        .animation(.snappy(duration: 0.2), value: tagsFocused)
    }

    /// Above the keyboard, drawn like the Markdown toolbar the sides have: one glass bar of the library's tags that
    /// match what's typed (the one Return takes in the accent colour), and Hide keyboard on its own. Typing something
    /// new says so, rather than showing nothing.
    private var suggestionBar: some View {
        GlassEffectContainer(spacing: 8) {
            HStack(spacing: 8) {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 0) {
                        ForEach(Array(suggestions.enumerated()), id: \.element) { i, tag in
                            suggestion("#\(tag)", primary: i == 0 && !draft.isEmpty) { addTag(tag) }
                        }
                        if suggestions.isEmpty {
                            let tag = normalizeTag(draft)
                            if tag.isEmpty {
                                Text("No tags yet").foregroundStyle(.tertiary).padding(.horizontal, 12)
                            } else {
                                suggestion("New tag #\(tag)", primary: true) { addTag(draft) }
                            }
                        }
                    }
                    .padding(.horizontal, 6)
                }
                .frame(height: barHeight)
                .clipShape(.capsule)
                .glassEffect(.regular.interactive(), in: .capsule)
                Button("Hide keyboard", systemImage: "keyboard.chevron.compact.down") { tagsFocused = false }
                    .labelStyle(.iconOnly)
                    .foregroundStyle(.primary)
                    .frame(width: barHeight, height: barHeight)
                    .contentShape(.circle)
                    .glassEffect(.regular.interactive(), in: .circle)
            }
            .font(.subheadline.weight(.medium))
            .buttonStyle(.plain)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 6)
    }

    private let barHeight: CGFloat = 40

    private func suggestion(_ title: String, primary: Bool, action: @escaping () -> Void) -> some View {
        Button(title, action: action)
            .foregroundStyle(primary ? AnyShapeStyle(palette.accent) : AnyShapeStyle(.primary))
            .fontWeight(primary ? .semibold : .medium)
            .padding(.horizontal, 12)
            .frame(maxHeight: .infinity)
            .contentShape(.rect)
    }

    private var suggestions: [String] {
        let have = Set(tags)
        let q = normalizeTag(draft)
        let all = libraryTags.map(\.tag).filter { !have.contains($0) }
        if q.isEmpty { return Array(all.prefix(8)) }
        let hits = all.filter { $0.contains(q) }
        return hits.filter { $0.hasPrefix(q) } + hits.filter { !$0.hasPrefix(q) }
    }

    private func addTag(_ raw: String) {
        let tag = normalizeTag(raw)
        draft = ""
        if !tag.isEmpty && !tags.contains(tag) { tags.append(tag) }
    }

    private func save() async {
        let content = joinSides(sides)
        guard !content.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            error = "Write something on the card first"
            return
        }
        var tags = tags
        let pending = normalizeTag(draft)
        if !pending.isEmpty && !tags.contains(pending) { tags.append(pending) }
        saving = true
        let deck = deck
        let done = await library.write { api in
            if let initial {
                try await api.updateCard(initial.id, deck: deck, content: content, tags: tags)
            } else {
                try await api.createCard(deck: deck, content: content, tags: tags)
            }
        }
        saving = false
        if done != nil { close() }
    }
}

/// A capsule button whose fill is drawn with its label, so the two always move as one. The system's bordered style
/// animates its own background on release, which slid away from a label that moved in the same update (Add side
/// moving down the card). Nothing about it animates: not the press, and not where it moves to.
private struct PillButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .foregroundStyle(.secondary)
            .padding(.horizontal, 12)
            .frame(height: 32)
            .background(.fill.tertiary, in: .capsule)
            .contentShape(.capsule)
            .opacity(configuration.isPressed ? 0.5 : 1)
            .transaction { $0.animation = nil }
    }
}

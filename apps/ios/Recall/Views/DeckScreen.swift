import SwiftUI

/// A deck's page (apps/web/src/DeckScreen.tsx): its name in the top bar with the deck's actions, the web's tool row
/// (view, tag filter, sort) pinned under it on glass, then its cards as a grid, list or column, with search and a new
/// card in the bottom toolbar.
struct DeckScreen: View {
    @Environment(Library.self) private var library
    @Environment(\.showSidebar) private var showSidebar
    @State private var editing: Editing?
    @State private var deleting: CardView?
    @State private var draft = ""
    /// The card open beside the list in the column view, on iPad.
    @State private var selected: String?
    @Environment(\.horizontalSizeClass) private var sizeClass

    var body: some View {
        Group {
            if let deck = library.deck {
                page(deck)
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .navigationDestination(for: CardView.self) { card in
            CardDetail(card: card) { editing = .edit(card) }
        }
        .sheet(item: $editing) { CardEditor(editing: $0) }
        .confirmationDialog(
            "Delete this card?", isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
            titleVisibility: .visible, presenting: deleting
        ) { card in
            Button("Delete card", role: .destructive) { delete(card) }
        }
        .onChange(of: library.deckId) {
            draft = ""
            selected = nil
        }
        #if DEBUG
        .task(id: library.cardsFor) {
            guard !scripted, let deck = library.deck, library.cardsFor == deck.id else { return }
            scripted = true
            switch Scripted.screen {
            case "new": editing = .new(deck: deck.id)
            case "editor": editing = library.cards.last.map(Editing.edit)
            default: break
            }
        }
        #endif
    }

    #if DEBUG
    @State private var scripted = false
    #endif

    private func page(_ deck: DeckSummary) -> some View {
        cards(deck)
            .background(Color(.systemGroupedBackground))
            .refreshable { await library.refresh() }
            .navigationTitle(deck.name)
            .navigationBarTitleDisplayMode(.inline)
            .searchable(text: $draft, prompt: "Search cards")
            .searchToolbarBehavior(.minimize)
            .safeAreaBar(edge: .top) {
                ToolRow(deck: deck.id)
                    .padding(.horizontal, 16)
                    .padding(.bottom, 10)
            }
            .toolbar {
                if let showSidebar {
                    ToolbarItem(placement: .topBarLeading) {
                        Button("Decks", systemImage: "sidebar.leading", action: showSidebar)
                    }
                }
                ToolbarItem(placement: .topBarTrailing) { DeckMenu(deck: deck) }

                DefaultToolbarItem(kind: .search, placement: .bottomBar)
                ToolbarSpacer(.flexible, placement: .bottomBar)
                ToolbarItem(placement: .bottomBar) {
                    Button("New card", systemImage: "square.and.pencil") { editing = .new(deck: deck.id) }
                }
            }
            .task(id: draft) {
                try? await Task.sleep(for: .milliseconds(150))
                if !Task.isCancelled { library.setSearch(draft) }
            }
    }

    @ViewBuilder
    private func cards(_ deck: DeckSummary) -> some View {
        let cards = library.cardsFor == deck.id ? library.cards : []
        if library.cardsFor != deck.id {
            ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if cards.isEmpty {
            ScrollView { empty(deck).padding(.top, 48) }
        } else if library.layout(deck.id) == .column {
            if sizeClass == .regular {
                columns(deck, cards)
            } else {
                List {
                    newCardRow(deck)
                    ForEach(cards) { card in
                        NavigationLink(value: card) { CompactRow(card: card) }
                            .contextMenu { menu(card) }
                            .swipeActions { swipe(card) }
                    }
                }
                .scrollContentBackground(.hidden)
            }
        } else {
            ScrollView {
                Group {
                    if library.layout(deck.id) == .grid {
                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 12)], spacing: 12) {
                            newTile(deck, list: false)
                            tiles(cards, list: false)
                        }
                    } else {
                        LazyVStack(spacing: 10) {
                            newTile(deck, list: true)
                            tiles(cards, list: true)
                        }
                    }
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 8)
            }
            .scrollDismissesKeyboard(.immediately)
        }
    }

    /// The column view with room for both: the cards down the side, the open one (the first, to start with) beside.
    private func columns(_ deck: DeckSummary, _ cards: [CardView]) -> some View {
        let open = cards.first { $0.id == selected } ?? cards.first
        return HStack(spacing: 0) {
            List(selection: Binding(get: { open?.id }, set: { selected = $0 })) {
                newCardRow(deck).selectionDisabled()
                ForEach(cards) { card in
                    CompactRow(card: card)
                        .tag(card.id)
                        .contextMenu { menu(card) }
                        .swipeActions { swipe(card) }
                }
            }
            .scrollContentBackground(.hidden)
            .frame(width: 340)
            Divider().ignoresSafeArea()
            ScrollView {
                if let open {
                    VStack(alignment: .trailing, spacing: 12) {
                        Button("Edit card", systemImage: "pencil") { editing = .edit(open) }
                            .buttonStyle(.glass)
                        CardSides(card: open)
                    }
                    .frame(maxWidth: 760)
                    .padding(20)
                    .frame(maxWidth: .infinity)
                }
            }
        }
    }

    /// "New card", ahead of the cards, as on the web.
    private func newTile(_ deck: DeckSummary, list: Bool) -> some View {
        Button { editing = .new(deck: deck.id) } label: {
            Label("New card", systemImage: "plus")
                .font(.body.weight(.medium))
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, minHeight: list ? 64 : 156)
                .overlay {
                    RoundedRectangle(cornerRadius: 16, style: .continuous)
                        .strokeBorder(.tertiary, style: StrokeStyle(lineWidth: 1, dash: [4, 4]))
                }
                .contentShape(.rect(cornerRadius: 16))
        }
        .buttonStyle(TileButtonStyle())
    }

    private func newCardRow(_ deck: DeckSummary) -> some View {
        Button("New card", systemImage: "plus") { editing = .new(deck: deck.id) }
    }

    private func tiles(_ cards: [CardView], list: Bool) -> some View {
        ForEach(cards) { card in
            Button { editing = .edit(card) } label: { Tile(card: card, list: list) }
                .buttonStyle(TileButtonStyle())
                .contextMenu { menu(card) }
        }
    }

    @ViewBuilder
    private func empty(_ deck: DeckSummary) -> some View {
        let q = library.search, tag = library.tag
        if !q.isEmpty || !tag.isEmpty {
            let what = [q.isEmpty ? nil : "“\(q)”", tag.isEmpty ? nil : "#\(tag)"].compactMap(\.self).joined(separator: " in ")
            ContentUnavailableView {
                Label("No cards match \(what)", systemImage: q.isEmpty ? "line.3.horizontal.decrease" : "magnifyingglass")
            } actions: {
                Button(!q.isEmpty && !tag.isEmpty ? "Clear search and tag" : !q.isEmpty ? "Clear search" : "Show all cards") {
                    draft = ""
                    library.clearFilters()
                }
                .buttonStyle(.glass)
            }
        } else {
            ContentUnavailableView {
                Label("No cards in \(deck.name) yet", systemImage: "rectangle.on.rectangle")
            } description: {
                Text("Write in Markdown. A line with only --- starts the next side.")
            } actions: {
                Button("Add card") { editing = .new(deck: deck.id) }.buttonStyle(.glassProminent)
            }
        }
    }

    @ViewBuilder
    private func menu(_ card: CardView) -> some View {
        Button("Edit card", systemImage: "pencil") { editing = .edit(card) }
        Button("Delete card", systemImage: "trash", role: .destructive) { deleting = card }
    }

    private func swipe(_ card: CardView) -> some View {
        Button("Delete", systemImage: "trash", role: .destructive) { delete(card) }
    }

    private func delete(_ card: CardView) {
        Task { await library.write { try await $0.deleteCard(card.id) } }
    }

}

enum Editing: Identifiable {
    case new(deck: String)
    case edit(CardView)

    var id: String {
        switch self {
        case .new(let deck): "new-\(deck)"
        case .edit(let card): card.id
        }
    }
}

/// Shows only the cards with one tag (apps/web/src/DeckScreen.tsx, `TagFilter`): the library's tags, this deck's
/// first.
private struct TagMenu: View {
    @Environment(Library.self) private var library
    let deck: String
    @State private var tags: [String] = []

    var body: some View {
        let tag = library.tag
        // A tag picked before stays pickable even if no card carries it any more.
        let options = !tag.isEmpty && !tags.contains(tag) ? [tag] + tags : tags
        Menu {
            Picker("Filter by tag", selection: Binding(get: { library.tag }, set: { library.setTag($0) })) {
                Text("All cards").tag("")
                ForEach(options, id: \.self) { Text("#\($0)").tag($0) }
            }
            .pickerStyle(.inline)
        } label: {
            // The icon, and the tag once one's picked, as on the web.
            HStack(spacing: 6) {
                Image(systemName: tag.isEmpty ? "tag" : "tag.fill")
                if !tag.isEmpty { Text("#\(tag)").lineLimit(1) }
            }
            .foregroundStyle(tag.isEmpty ? AnyShapeStyle(.primary) : AnyShapeStyle(.tint))
            .padding(.horizontal, 13)
            .frame(minWidth: 46, minHeight: 42)
            .contentShape(.rect)
            .accessibilityLabel(tag.isEmpty ? "Filter by tag" : "Only cards tagged \(tag)")
        }
        .task(id: "\(deck) \(library.refreshed?.timeIntervalSince1970 ?? 0)") {
            guard let core = library.core else { return }
            if let fetched = try? await core.tags(deck: deck) { tags = fetched.map(\.tag) }
        }
    }
}

/// A card's front as a tile, with how many sides it hides and when it's due. In the list view it runs the page's
/// width and shows more of the front.
private struct Tile: View {
    let card: CardView
    var list = false

    var body: some View {
        let front = card.sides.first ?? ""
        let cap: CGFloat = list ? 200 : 132
        VStack(alignment: .leading, spacing: 0) {
            MarkdownView(text: front)
                .modifier(FrontFont(size: FrontSize(front)))
                .foregroundStyle(.primary)
                .multilineTextAlignment(.leading)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, maxHeight: cap, alignment: .topLeading)
                .clipped()
                // Fades only where a long front reaches the cap.
                .mask(alignment: .top) {
                    VStack(spacing: 0) {
                        Color.black.frame(height: cap - 28)
                        LinearGradient(colors: [.black, .clear], startPoint: .top, endPoint: .bottom).frame(height: 28)
                    }
                }
            Spacer(minLength: 10)
            Footer(card: card)
        }
        .padding(list ? 18 : 14)
        .frame(maxWidth: .infinity, minHeight: list ? 0 : 156, alignment: .topLeading)
        .cardSurface()
        .contentShape(.rect(cornerRadius: 16))
    }
}

/// "+1 side" and when it's due, in small caps under a dashed rule, beneath a card's front.
private struct Footer: View {
    let card: CardView

    var body: some View {
        HStack {
            if card.sides.count > 1 {
                // Just the count where the due label leaves no room for the words.
                ViewThatFits {
                    Text("+ \(hiddenSides(card.sides.count - 1))")
                    Text("+ \(card.sides.count - 1)")
                }
                .lineLimit(1)
            }
            Spacer(minLength: 8)
            DueLabel(card: card).lineLimit(1).fixedSize()
        }
        .font(.caption2.weight(.semibold).monospacedDigit())
        .textCase(.uppercase)
        .tracking(0.6)
        .foregroundStyle(.tertiary)
        .padding(.top, 8)
        .overlay(alignment: .top) {
            Line().stroke(style: StrokeStyle(lineWidth: 1, dash: [3, 3])).foregroundStyle(.quaternary).frame(height: 1)
        }
    }
}

struct Line: Shape {
    func path(in rect: CGRect) -> Path {
        Path { $0.move(to: CGPoint(x: 0, y: rect.midY)); $0.addLine(to: CGPoint(x: rect.maxX, y: rect.midY)) }
    }
}

/// Short fronts render big, as tiles do on the desktop.
private struct FrontFont: ViewModifier {
    let size: FrontSize

    func body(content: Content) -> some View {
        switch size {
        case .xl: content.font(.title.weight(.medium))
        case .l: content.font(.title3)
        case .m, .s: content.font(.body)
        }
    }
}

private struct CompactRow: View {
    let card: CardView

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            let front = plain(card.sides.first ?? "")
            Text(front.isEmpty ? "Empty card" : front)
                .font(.body)
                .lineLimit(2)
            DueLabel(card: card).font(.caption).foregroundStyle(.secondary)
        }
        .padding(.vertical, 2)
    }
}

/// A card pushed from the column view on iPhone, all its sides at once.
private struct CardDetail: View {
    let card: CardView
    let edit: () -> Void

    var body: some View {
        ScrollView { CardSides(card: card).padding(16) }
            .background(Color(.systemGroupedBackground))
            .navigationTitle(plain(card.sides.first ?? ""))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) { Button("Edit card", systemImage: "pencil", action: edit) }
            }
    }
}

/// A card's sides one under another, divided by rules, then how its reviews have gone and its tags
/// (apps/web/src/DeckScreen.tsx, `Detail`).
private struct CardSides: View {
    let card: CardView

    var body: some View {
        let s = card.schedule
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(card.sides.enumerated()), id: \.offset) { i, side in
                if i > 0 {
                    Line()
                        .stroke(style: StrokeStyle(lineWidth: 1, dash: [3, 4]))
                        .foregroundStyle(.tertiary)
                        .frame(height: 1)
                        .padding(.vertical, 20)
                        .padding(.horizontal, -20)
                }
                MarkdownView(text: side, selectable: true).font(.title2).lineSpacing(4)
            }
            VStack(alignment: .leading, spacing: 10) {
                Group {
                    if s.state == .new {
                        Text("New, not reviewed yet")
                    } else {
                        let history = [s.reps > 0 ? "✓ \(reviews(s.reps))" : nil, s.lapses > 0 ? lapses(s.lapses) : nil]
                            .compactMap(\.self)
                        let prefix = history.isEmpty ? "" : history.joined(separator: " · ") + " · "
                        if s.due <= nowMs() {
                            Text("\(prefix)\(Text("Due now").foregroundStyle(.tint))")
                        } else {
                            Text(prefix + "Due \(until(s.due))")
                        }
                    }
                }
                .font(.caption.weight(.semibold).monospacedDigit())
                .textCase(.uppercase)
                .tracking(0.6)
                .foregroundStyle(.secondary)
                if !card.tags.isEmpty {
                    FlowLayout { ForEach(card.tags, id: \.self) { TagChip(tag: $0) } }
                }
            }
            .padding(.top, 24)
        }
        .padding(20)
        .frame(maxWidth: .infinity, alignment: .leading)
        .cardSurface(radius: 18)
    }
}

/// A group of tools on a Liquid Glass capsule.
private struct ToolSurface: ViewModifier {
    func body(content: Content) -> some View {
        content.glassEffect(.regular.interactive(), in: .capsule)
    }
}

/// The web's tool row (apps/web/src/DeckScreen.tsx, `Tools`): the view as three segments, the tag filter, and on the
/// right the sort with its direction beside it.
private struct ToolRow: View {
    @Environment(Library.self) private var library
    let deck: String

    var body: some View {
        GlassEffectContainer(spacing: 8) {
            HStack(spacing: 8) {
                views
                TagMenu(deck: deck).modifier(ToolSurface())
                Spacer(minLength: 0)
                sort
            }
        }
        .buttonStyle(.plain)
        .foregroundStyle(.primary)
    }

    private var views: some View {
        let layout = library.layout(deck)
        return HStack(spacing: 2) {
            ForEach(CardLayout.allCases) { option in
                Button { library.setLayout(deck, option) } label: {
                    Image(systemName: option.icon)
                        .frame(width: 38, height: 36)
                        .background { if option == layout { Capsule().fill(.fill.secondary) } }
                        .contentShape(.rect)
                }
                .accessibilityLabel(option.label)
                .accessibilityAddTraits(option == layout ? .isSelected : [])
            }
        }
        .padding(3)
        .modifier(ToolSurface())
        .animation(.snappy(duration: 0.2), value: layout)
    }

    private var sort: some View {
        let sort = library.sort(deck)
        let direction = sort.by.directions.map { sort.desc ? $0.1 : $0.0 } ?? "Shuffle again"
        return HStack(spacing: 0) {
            Menu {
                Picker("Sort by", selection: Binding(get: { sort.by }, set: { library.setSort(deck, $0) })) {
                    ForEach(Sort.allCases) { Text($0.label).tag($0) }
                }
                .pickerStyle(.inline)
            } label: {
                HStack(spacing: 7) {
                    Image(systemName: "line.3.horizontal.decrease").foregroundStyle(.secondary)
                    Text(sort.by.short).lineLimit(1).fixedSize()
                }
                .padding(.leading, 13)
                .padding(.trailing, 10)
                .frame(height: 42)
                .contentShape(.rect)
            }
            .accessibilityLabel("Sort by \(sort.by.label.lowercased())")
            Rectangle().fill(.quaternary).frame(width: 1, height: 20)
            Button {
                if sort.by.directions == nil { library.setSort(deck, .random) } else { library.setSort(deck, sort.by, desc: !sort.desc) }
            } label: {
                Image(systemName: sort.by.directions == nil ? "shuffle" : sort.desc ? "arrow.down" : "arrow.up")
                    .frame(width: 44, height: 42)
                    .contentShape(.rect)
            }
            .accessibilityLabel(direction)
        }
        .modifier(ToolSurface())
    }
}

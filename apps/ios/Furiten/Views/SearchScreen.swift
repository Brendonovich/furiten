import SwiftUI

/// Cards from every deck whose content matches the search, as tiles under their deck's name; tapping one edits it, and
/// Review in the bottom toolbar crams them, deck by deck as shown. A layer over whatever the sidebar has open (`Shell`):
/// Close, or cancelling the search with nothing typed, calls `close`, back to it as it was.
struct SearchScreen: View {
    @Environment(Library.self) private var library
    @Environment(\.palette) private var palette
    @Environment(\.showSidebar) private var showSidebar
    @State private var draft = ""
    @State private var focused = true
    /// The results, and the search they're for.
    @State private var results: (query: String, cards: [CardView])?
    @State private var editing: Editing?
    @State private var deleting: CardView?
    @State private var cramming: Cram?
    let close: () -> Void

    private var query: String { draft.trimmingCharacters(in: .whitespaces) }

    var body: some View {
        content
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(palette.page)
            .navigationTitle("Search")
            .navigationBarTitleDisplayMode(.inline)
            .searchable(text: $draft, isPresented: $focused, prompt: "Search cards in every deck")
            .toolbar {
                if let showSidebar {
                    ToolbarItem(placement: .topBarLeading) {
                        Button("Decks", systemImage: "sidebar.leading", action: showSidebar)
                    }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Close search", systemImage: "xmark", action: close)
                }
                DefaultToolbarItem(kind: .search, placement: .bottomBar)
                ToolbarSpacer(.flexible, placement: .bottomBar)
                ToolbarItem(placement: .bottomBar) {
                    CramButton(cards: shown) { cramming = Cram(title: "“\(query)”", deck: nil, cards: shown) }
                }
            }
            // Cancelled with nothing typed: there's nothing to come back to, so back to the page.
            .onChange(of: focused) { _, active in
                if !active && query.isEmpty && cramming == nil && editing == nil { close() }
            }
            .cramSheet($cramming)
            #if DEBUG
            .onAppear { if let query = Scripted.query { draft = query } }
            #endif
            .sheet(item: $editing) { CardEditor(editing: $0) }
            .confirmationDialog(
                "Delete this card?", isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
                titleVisibility: .visible, presenting: deleting
            ) { card in
                Button("Delete card", role: .destructive) { delete(card) }
            }
            .task(id: "\(query) \(library.refreshed?.timeIntervalSince1970 ?? 0)") {
                let query = query
                guard !query.isEmpty else { return results = nil }
                try? await Task.sleep(for: .milliseconds(150))
                guard !Task.isCancelled, let core = library.core else { return }
                do {
                    let cards = try await core.cards(
                        deck: nil, query: query, tag: "", sort: SortState(by: .updated, desc: true, seed: 0))
                    if !Task.isCancelled { results = (query, cards) }
                } catch {
                    library.fail(error)
                }
            }
    }

    @ViewBuilder
    private var content: some View {
        if query.isEmpty {
            ContentUnavailableView(
                "Search cards", systemImage: "magnifyingglass", description: Text("Find cards in every deck."))
        } else if let results, results.query == query {
            if results.cards.isEmpty {
                ContentUnavailableView.search(text: query)
            } else {
                list(results.cards)
            }
        } else {
            ProgressView()
        }
    }

    /// The results as they're shown: grouped by deck, in the sidebar's order.
    private var shown: [CardView] {
        guard let results, results.query == query else { return [] }
        let byDeck = Dictionary(grouping: results.cards, by: \.deckId)
        return deckTree(library.decks).flatMap { byDeck[$0.deck.id] ?? [] }
    }

    /// The cards grouped by deck, in the sidebar's order.
    private func list(_ cards: [CardView]) -> some View {
        let byDeck = Dictionary(grouping: cards, by: \.deckId)
        let decks = deckTree(library.decks).map(\.deck).filter { byDeck[$0.id] != nil }
        return ScrollView {
            LazyVStack(alignment: .leading, spacing: 20) {
                ForEach(decks) { deck in
                    VStack(alignment: .leading, spacing: 10) {
                        Label(deck.name, systemImage: "book.closed")
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(.secondary)
                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 12)], spacing: 12) {
                            ForEach(byDeck[deck.id] ?? []) { card in
                                Button { editing = .edit(card) } label: { Tile(card: card) }
                                    .buttonStyle(TileButtonStyle())
                                    .contextMenu {
                                        Button("Edit card", systemImage: "pencil") { editing = .edit(card) }
                                        Button("Delete card", systemImage: "trash", role: .destructive) { deleting = card }
                                    }
                            }
                        }
                    }
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
        }
        .scrollDismissesKeyboard(.immediately)
    }

    private func delete(_ card: CardView) {
        Task { await library.write { try await $0.deleteCard(card.id) } }
    }
}

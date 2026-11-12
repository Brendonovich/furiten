import SwiftUI

/// Every card with a tag, from every deck (apps/web/src/TagScreen.tsx): a deck's page but for the tag filter and New
/// card, with the same tool row (view, sort) and search; tapping a card edits it, and Review crams what's listed.
struct TagScreen: View {
    @Environment(Library.self) private var library
    @Environment(\.palette) private var palette
    @Environment(\.showSidebar) private var showSidebar
    let tag: String
    @State private var cards: [CardView]?
    @State private var draft = ""
    @State private var search = ""
    @State private var editing: Editing?
    @State private var cramming: Cram?
    /// The card open beside the list in the column view, on iPad.
    @State private var selected: String?

    var body: some View {
        let sort = library.sort(tagKey(tag))
        content
            .background(palette.page)
            .refreshable { await library.refresh() }
            .navigationTitle("#\(tag)")
            .navigationBarTitleDisplayMode(.inline)
            .searchable(text: $draft, prompt: "Search cards")
            .searchToolbarBehavior(.minimize)
            .safeAreaBar(edge: .top) {
                ToolRow(page: tagKey(tag))
                    .padding(.horizontal, 16)
                    .padding(.bottom, 10)
            }
            .toolbar {
                if let showSidebar {
                    ToolbarItem(placement: .topBarLeading) {
                        Button("Decks", systemImage: "sidebar.leading", action: showSidebar)
                    }
                }

                DefaultToolbarItem(kind: .search, placement: .bottomBar)
                ToolbarSpacer(.flexible, placement: .bottomBar)
                ToolbarItem(placement: .bottomBar) {
                    CramButton(cards: cards ?? []) { cramming = Cram(title: "#\(tag)", deck: nil, cards: cards ?? []) }
                }
                ToolbarSpacer(.flexible, placement: .bottomBar)
            }
            .navigationDestination(for: CardView.self) { card in
                CardDetail(card: card) { editing = .edit(card) }
            }
            .sheet(item: $editing) { CardEditor(editing: $0) }
            .cramSheet($cramming)
            .task(id: draft) {
                try? await Task.sleep(for: .milliseconds(150))
                if !Task.isCancelled { search = draft }
            }
            .task(id: "\(search) \(sort.by.rawValue) \(sort.desc) \(sort.seed) \(library.refreshed?.timeIntervalSince1970 ?? 0)") {
                guard let core = library.core else { return }
                do {
                    let found = try await core.cards(
                        deck: nil, query: search.trimmingCharacters(in: .whitespaces), tag: tag, sort: sort)
                    if !Task.isCancelled { cards = found }
                } catch {
                    library.fail(error)
                }
            }
    }

    @ViewBuilder
    private var content: some View {
        if let cards {
            if cards.isEmpty {
                ScrollView { empty.padding(.top, 48) }
            } else {
                CardList(cards: cards, layout: library.layout(tagKey(tag)), selected: $selected, editing: $editing)
            }
        } else {
            ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    @ViewBuilder
    private var empty: some View {
        if search.isEmpty {
            ContentUnavailableView("No cards tagged #\(tag)", systemImage: "number")
        } else {
            ContentUnavailableView {
                Label("No cards match “\(search)” in #\(tag)", systemImage: "magnifyingglass")
            } actions: {
                Button("Clear search") {
                    draft = ""
                    search = ""
                }
                .buttonStyle(.glass)
            }
        }
    }
}

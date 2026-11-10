import SwiftUI

/// Renaming, nesting, moving and deleting a deck (apps/web/src/DeckActions.tsx), as the toolbar's menu.
struct DeckMenu: View {
    @Environment(Library.self) private var library
    let deck: DeckSummary

    @State private var prompt = Prompt.rename
    @State private var prompting = false
    @State private var name = ""
    @State private var confirmDelete = false

    private enum Prompt { case rename, subdeck }

    var body: some View {
        Menu("Deck actions", systemImage: "ellipsis") {
            Section("\(deck.cards) cards") {
                Button("Rename", systemImage: "pencil") {
                    name = deck.name
                    prompt = .rename
                    prompting = true
                }
                Button("New subdeck", systemImage: "arrow.turn.down.right") {
                    name = ""
                    prompt = .subdeck
                    prompting = true
                }
                Menu("Move to…", systemImage: "book.closed") {
                    Picker("Move to", selection: Binding(get: { deck.parentId }, set: { move($0) })) {
                        Text("Top level").tag(String?.none)
                        ForEach(targets, id: \.deck.id) { row in
                            Text(deckPath(library.decks, row.deck.id)).tag(Optional(row.deck.id))
                        }
                    }
                }
            }
            Button("Delete deck", systemImage: "trash", role: .destructive) { confirmDelete = true }
        }
        .alert(
            prompt == .rename ? "Rename deck" : "New deck in \(deck.name)",
            isPresented: $prompting
        ) {
            TextField("Name", text: $name)
            Button("Cancel", role: .cancel) {}
            Button("Save", action: save)
        }
        .confirmationDialog("Delete \(deck.name)?", isPresented: $confirmDelete, titleVisibility: .visible) {
            Button("Delete deck and its cards", role: .destructive) {
                // Off to its parent first; a top-level deck leaves for Review once it's gone from the list.
                if let parent = deck.parentId { library.selectDeck(parent) }
                Task { await library.write { try await $0.deleteDeck(deck.id) } }
            }
        } message: {
            Text("Its subdecks and all their cards are deleted too.")
        }
    }

    /// Where it can move: any deck that isn't it or under it.
    private var targets: [(deck: DeckSummary, depth: Int)] {
        var under: Set<String> = [deck.id]
        let tree = deckTree(library.decks)
        for row in tree where row.deck.parentId.map(under.contains) == true { under.insert(row.deck.id) }
        return tree.filter { !under.contains($0.deck.id) }
    }

    private func move(_ parent: String?) {
        guard parent != deck.parentId else { return }
        Task { await library.write { try await $0.moveDeck(deck.id, parent: parent) } }
    }

    private func save() {
        let value = name.trimmingCharacters(in: .whitespaces)
        guard !value.isEmpty else { return }
        Task {
            switch prompt {
            case .rename:
                await library.write { try await $0.renameDeck(deck.id, name: value) }
            case .subdeck:
                if let created = await library.write({ try await $0.createDeck(name: value, parent: deck.id) }) {
                    library.selectDeck(created.id)
                }
            }
        }
    }
}

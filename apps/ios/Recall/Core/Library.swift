import Observation
import UIKit

/// What the screens show, and the UI's own state (the web app's apps/web/src/state.ts). Data comes from this phone's
/// own core, which syncs with your computer (or Recall Cloud) in the background; it's refetched after every write and
/// whenever the core reports a change, including changes that arrive through sync. Nothing here decides scheduling
/// or order.
@MainActor
@Observable
final class Library {
    /// The linked core; nil only if the library couldn't be opened.
    @ObservationIgnored let core: Core?
    private(set) var decks: [DeckSummary] = []
    private(set) var loaded = false
    private(set) var deckId: String?
    private(set) var cards: [CardView] = []
    private(set) var cardsFor: String?
    private(set) var search = ""
    /// Only cards with this tag, or every card when empty.
    private(set) var tag = ""
    /// When the screens last refetched, which views key their own fetches on.
    private(set) var refreshed: Date?
    /// Where the library syncs, and how that's going.
    private(set) var sync: CoreStatus?
    var notice: String?

    private var sorts: [String: SortState] = [:]
    private var layouts: [String: CardLayout] = [:]
    /// A saved random sort's order for this launch, so it holds across refreshes until it's shuffled again.
    @ObservationIgnored private let seed = UInt32.random(in: 0...UInt32.max)
    /// A refetch for the core's `changed`, so a burst of them (a sync pulling many changes) refetches once.
    @ObservationIgnored private var pending: Task<Void, Never>?

    init() {
        let relay = Relay()
        do {
            core = try Core { relay.changed() }
        } catch {
            core = nil
            notice = "Couldn’t open your library: \(error.localizedDescription)"
        }
        deckId = defaults.string(forKey: Keys.deck)
        relay.library = self
        Task {
            #if DEBUG
            // `-pair <ticket>` on launch pairs with a computer, for scripted runs (`simctl launch`, `devicectl … launch`).
            if let ticket = UserDefaults.standard.string(forKey: "pair").flatMap(pairingTicket),
               (try? await core?.status())?.signedIn != true
            {
                do { try await pair(ticket: ticket) } catch { fail(error) }
            }
            #endif
            await refresh()
        }
        watchSync()
    }

    /// Keeps `sync` current while the app is open: a connection to the computer comes and goes without a change.
    private func watchSync() {
        Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(3))
                guard let self, let core = self.core else { return }
                if let status = try? await core.status(), status != self.sync { self.sync = status }
            }
        }
    }

    var deck: DeckSummary? { decks.first { $0.id == deckId } }

    func fail(_ error: Error) {
        if !(error is CancellationError) {
            notice = error.localizedDescription
        }
    }

    /// The core changed: something here wrote, or a sync brought changes in.
    fileprivate func changed() {
        pending?.cancel()
        pending = Task {
            try? await Task.sleep(for: .milliseconds(100))
            if !Task.isCancelled { await refresh() }
        }
    }

    // MARK: Fetching

    /// Refetches the decks; true when the open deck changed and its cards were refetched too.
    @discardableResult
    func refreshDecks() async -> Bool {
        guard let core else { return false }
        do {
            sync = try? await core.status()
            let decks = try await core.decks()
            let open = deckId
            // A deck that's gone, deleted here or on another device, leaves for Review (the screens follow `deckId`).
            let deck: String? =
                if let open { decks.contains { $0.id == open } ? open : nil } else { deckTree(decks).first?.deck.id }
            self.decks = decks
            loaded = true
            refreshed = .now
            deckId = deck
            defaults.set(deck, forKey: Keys.deck)
            if deck == open { return false }
            search = ""
            tag = ""
            await refreshCards()
            return true
        } catch {
            fail(error)
            return false
        }
    }

    /// Fetches the open deck's cards for its search, tag and sort.
    func refreshCards() async {
        guard let core, let deck = deckId else { return }
        let (search, tag, sort) = (search, tag, sort(deck))
        do {
            let cards = try await core.cards(
                deck: deck, query: search.trimmingCharacters(in: .whitespaces), tag: tag, sort: sort)
            // A slower, older request mustn't overwrite a newer one.
            guard deck == deckId, search == self.search, tag == self.tag, sort == self.sort(deck) else { return }
            self.cards = cards
            cardsFor = deck
        } catch {
            fail(error)
        }
    }

    func refresh() async {
        if !(await refreshDecks()) { await refreshCards() }
    }

    /// Runs a write, then refetches everything it may have changed.
    @discardableResult
    func write<T: Sendable>(_ f: (Core) async throws -> T) async -> T? {
        guard let core else { return nil }
        do {
            let result = try await f(core)
            await refresh()
            return result
        } catch {
            fail(error)
            return nil
        }
    }

    // MARK: What's on screen

    func selectDeck(_ id: String) {
        if cardsFor != id { cards = [] }
        deckId = id
        search = ""
        tag = ""
        defaults.set(id, forKey: Keys.deck)
        Task { await refreshCards() }
    }

    func setSearch(_ q: String) {
        guard q != search else { return }
        search = q
        Task { await refreshCards() }
    }

    func setTag(_ t: String) {
        guard t != tag else { return }
        tag = t
        Task { await refreshCards() }
    }

    /// Back to every card in the deck: no search, no tag.
    func clearFilters() {
        guard !search.isEmpty || !tag.isEmpty else { return }
        search = ""
        tag = ""
        Task { await refreshCards() }
    }

    func layout(_ deck: String) -> CardLayout {
        layouts[deck] ?? defaults.string(forKey: Keys.layout(deck)).flatMap(CardLayout.init) ?? .grid
    }

    func setLayout(_ deck: String, _ layout: CardLayout) {
        layouts[deck] = layout
        defaults.set(layout.rawValue, forKey: Keys.layout(deck))
    }

    /// The deck's sort, as last picked there; a random sort shuffles afresh each launch.
    func sort(_ deck: String) -> SortState {
        if let sort = sorts[deck] { return sort }
        let saved = (defaults.string(forKey: Keys.sort(deck)) ?? "created").split(separator: "-")
        return SortState(
            by: saved.first.flatMap { Sort(rawValue: String($0)) } ?? .created,
            desc: saved.count > 1 && saved[1] == "desc", seed: seed)
    }

    /// Picking the current sort keeps its direction; picking another starts in its default direction.
    func setSort(_ deck: String, _ by: Sort, desc: Bool? = nil) {
        let current = sort(deck)
        let next = SortState(
            by: by, desc: desc ?? (by == current.by ? current.desc : by.descendingFirst),
            seed: by == .random ? .random(in: 0...UInt32.max) : current.seed)
        sorts[deck] = next
        defaults.set(next.desc ? "\(by.rawValue)-desc" : by.rawValue, forKey: Keys.sort(deck))
        Task { await refreshCards() }
    }

    // MARK: Syncing

    /// Pairs with a computer by the ticket its Settings show (or `recall host pair` prints), then syncs.
    func pair(ticket: String) async throws {
        guard let core else { return }
        try await core.pair(ticket: ticket, name: UIDevice.current.name)
        notice = nil
        await refresh()
    }

    /// Signs in to Recall Cloud, creating an account without a token. Returns the account's token.
    @discardableResult
    func signIn(server: String, token: String?) async throws -> String {
        guard let core else { return "" }
        let token = try await core.signIn(server: server, token: token)
        notice = nil
        await refresh()
        return token
    }

    /// Stops syncing. Everything stays on this phone.
    func signOut() async {
        await write { try await $0.signOut() }
    }

    func syncNow() async {
        await write { try await $0.syncNow() }
    }

    /// The app came back to the foreground: sync, and catch up.
    func resume() {
        core?.syncSoon()
        Task { await refresh() }
    }

    /// The app is going to the background: push what's left while iOS gives it a moment.
    func suspend() {
        guard let core, sync?.signedIn == true else { return }
        let task = UIApplication.shared.beginBackgroundTask(withName: "Sync")
        Task {
            _ = try? await core.syncNow()
            UIApplication.shared.endBackgroundTask(task)
        }
    }

    private var defaults: UserDefaults { .standard }

    private enum Keys {
        static let deck = "recall.deck"
        static func layout(_ deck: String) -> String { "recall.view.\(deck)" }
        static func sort(_ deck: String) -> String { "recall.sort.\(deck)" }
    }
}

/// Carries the core's `changed`, from a core thread, to the library on the main actor.
private final class Relay: @unchecked Sendable {
    @MainActor weak var library: Library?

    func changed() {
        Task { @MainActor in self.library?.changed() }
    }
}

struct SortState: Equatable, Sendable {
    var by: Sort
    var desc: Bool
    var seed: UInt32
}

/// The deck tree: each deck followed by its subdecks, alphabetical at every level; orphans go to the top.
func deckTree(_ decks: [DeckSummary]) -> [(deck: DeckSummary, depth: Int)] {
    let ids = Set(decks.map(\.id))
    var children: [String?: [DeckSummary]] = [:]
    for d in decks {
        let parent = d.parentId.flatMap { ids.contains($0) ? $0 : nil }
        children[parent, default: []].append(d)
    }
    var out: [(deck: DeckSummary, depth: Int)] = []
    func walk(_ parent: String?, _ depth: Int) {
        for d in (children[parent] ?? []).sorted(by: { $0.name.localizedStandardCompare($1.name) == .orderedAscending }) {
            out.append((d, depth))
            walk(d.id, depth + 1)
        }
    }
    walk(nil, 0)
    return out
}

/// Cards to review across every deck: the top-level decks' counts, which include their subdecks'.
func inboxCount(_ decks: [DeckSummary]) -> Int {
    let ids = Set(decks.map(\.id))
    return decks.filter { $0.parentId.map { !ids.contains($0) } ?? true }.reduce(0) { $0 + $1.due + $1.new }
}

/// A deck's name with its parents': "Languages / Spanish".
func deckPath(_ decks: [DeckSummary], _ id: String) -> String {
    var names: [String] = []
    var at: String? = id
    var seen = Set<String>()
    while let id = at, !seen.contains(id), let d = decks.first(where: { $0.id == id }) {
        seen.insert(id)
        names.insert(d.name, at: 0)
        at = d.parentId
    }
    return names.joined(separator: " / ")
}

#if DEBUG
/// Launch arguments for scripted runs (`apps/ios/scripts/simulate.sh`).
enum Scripted {
    /// `-screen inbox|decks|settings|deck|new|editor` opens that screen; the deck's screens once it has loaded.
    /// With none, or `inbox`, the app opens on Review.
    static var screen: String? { UserDefaults.standard.string(forKey: "screen") }
    /// `-reveal YES` shows every side of each card under review, for screenshots of the answers.
    static var reveal: Bool { UserDefaults.standard.bool(forKey: "reveal") }
    /// `-unfocus 3` hides the editor's keyboard that many seconds after it opens, to record unfocusing.
    static var unfocus: Double { UserDefaults.standard.double(forKey: "unfocus") }
}
#endif

import Foundation
import RecallCore

struct CoreError: Error, LocalizedError {
    let code: String
    let message: String

    var errorDescription: String? { message }
}

/// Recall's core, linked in (`crates/ffi`): this phone's own library in SQLite, with FSRS and sync, answering the
/// local core protocol (docs/contract.md, "Local core protocol"). It holds no domain logic of its own: queues,
/// schedules, sorting and tag ordering all come from the core, as on the desktop.
final class Core: @unchecked Sendable {
    private let handle: Handle
    private let listener: Unmanaged<Listener>

    /// Opens the library in Application Support's `Recall` directory. `changed` runs on a core thread after any
    /// write, including writes that arrive through sync.
    init(changed: @escaping @Sendable () -> Void) throws {
        let home = URL.applicationSupportDirectory.appending(path: "Recall", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        let listener = Unmanaged.passRetained(Listener(changed))
        var error: UnsafeMutablePointer<CChar>?
        let opened = home.path(percentEncoded: false).withCString { path in
            recall_open(path, { ctx in
                guard let ctx else { return }
                Unmanaged<Listener>.fromOpaque(ctx).takeUnretainedValue().changed()
            }, listener.toOpaque(), &error)
        }
        guard let opened else {
            listener.release()
            let detail = error.flatMap { e -> CoreError? in
                defer { recall_string_free(e) }
                return try? decoder.decode(CoreError.Body.self, from: Data(String(cString: e).utf8)).error
            }
            throw detail ?? CoreError(code: "internal", message: "Couldn’t open your library")
        }
        handle = Handle(raw: opened)
        self.listener = listener
    }

    deinit {
        recall_close(handle.raw)
        listener.release()
    }

    /// Asks the core to sync now if it's signed in, as when the app comes back to the foreground.
    func syncSoon() {
        recall_sync_soon(handle.raw)
    }

    // MARK: Library

    func decks() async throws -> [DeckSummary] {
        try await call("decks.list")
    }

    func createDeck(name: String, parent: String?) async throws -> Deck {
        try await call("decks.create", DeckMove(id: nil, name: name, parentId: parent))
    }

    func renameDeck(_ id: String, name: String) async throws -> Deck {
        try await call("decks.update", DeckRename(id: id, name: name))
    }

    func moveDeck(_ id: String, parent: String?) async throws -> Deck {
        try await call("decks.update", DeckMove(id: id, name: nil, parentId: parent))
    }

    func deleteDeck(_ id: String) async throws {
        let _: Ack = try await call("decks.delete", ID(id: id))
    }

    func cards(deck: String, query: String, tag: String, sort: SortState) async throws -> [CardView] {
        try await call("cards.list", CardQuery(
            deckId: deck, q: query.isEmpty ? nil : query, tag: tag.isEmpty ? nil : tag, limit: 500,
            sort: sort.by.rawValue, desc: sort.desc ? true : nil, seed: sort.by == .random ? sort.seed : nil))
    }

    func createCard(deck: String, content: String, tags: [String]) async throws -> CardView {
        try await call("cards.create", CardBody(id: nil, content: content, tags: tags, deckId: deck))
    }

    func updateCard(_ id: String, deck: String, content: String, tags: [String]) async throws -> CardView {
        try await call("cards.update", CardBody(id: id, content: content, tags: tags, deckId: deck))
    }

    func deleteCard(_ id: String) async throws {
        let _: Ack = try await call("cards.delete", ID(id: id))
    }

    func tags(deck: String?) async throws -> [TagSummary] {
        try await call("tags.list", DeckFilter(deckId: deck))
    }

    func next(deck: String?) async throws -> NextReview {
        try await call("review.next", DeckFilter(deckId: deck))
    }

    func answer(_ card: String, _ recalled: Recalled, durationMs: Int64) async throws -> CardView {
        try await call("review.answer", Answer(cardId: card, remembered: recalled == .remembered, durationMs: durationMs))
    }

    // MARK: Sync

    func status() async throws -> CoreStatus {
        try await call("status")
    }

    /// Pairs with a computer (or a self-hosted hub) by its ticket, and syncs with it from now on.
    func pair(ticket: String, name: String) async throws {
        let _: Paired = try await call("sync.login", HubLogin(hub: ticket, name: name))
    }

    /// Signs in to Recall Cloud; without a token, creates an account. Returns the account's token.
    func signIn(server: String, token: String?) async throws -> String {
        let r: LoggedIn = try await call("sync.login", CloudLogin(server: server, token: token))
        return r.token
    }

    @discardableResult
    func syncNow() async throws -> SyncResult {
        try await call("sync.now")
    }

    /// Stops syncing; the library stays on this phone.
    func signOut() async throws {
        let _: Ack = try await call("sync.logout")
    }

    // MARK: Calls

    /// One request, answered off the main thread: the core blocks while it works (a sync, say).
    private func call<R: Decodable & Sendable>(_ method: String, _ params: (any Encodable & Sendable)? = nil)
        async throws -> R
    {
        let line = try encoder.encode(Request(method: method, params: params.map(AnyParams.init)))
        let reply: Data = await withCheckedContinuation { done in
            DispatchQueue.global(qos: .userInitiated).async { [handle] in
                let text = String(decoding: line, as: UTF8.self)
                let out = text.withCString { recall_request(handle.raw, $0) }
                defer { recall_string_free(out) }
                done.resume(returning: out.map { Data(String(cString: $0).utf8) } ?? Data())
            }
        }
        try Task.checkCancellation()
        let decoded: Reply<R>
        do {
            decoded = try decoder.decode(Reply<R>.self, from: reply)
        } catch {
            throw CoreError(code: "internal", message: "The core sent something unexpected: \(error)")
        }
        if let e = decoded.error { throw e }
        guard let result = decoded.result else { throw CoreError(code: "internal", message: "No answer from the core") }
        return result
    }
}

/// The core is safe to call from any thread (docs/contract.md, "Mobile").
private struct Handle: @unchecked Sendable {
    let raw: OpaquePointer
}

private final class Listener: @unchecked Sendable {
    let changed: @Sendable () -> Void
    init(_ changed: @escaping @Sendable () -> Void) { self.changed = changed }
}

extension CoreError: Decodable {
    struct Body: Decodable { let error: CoreError }
}

/// Where this library syncs, from the core's `status`.
struct CoreStatus: Decodable, Equatable, Sendable {
    let signedIn: Bool
    let lastSync: Int64?
    /// `hub:<endpoint id>` when paired with a computer or a self-hosted hub, else Recall Cloud's URL.
    let server: String?
    let syncError: String?
    /// With a computer or hub, whether the connection to it is open.
    let connected: Bool?

    var paired: Bool { server?.hasPrefix("hub:") ?? false }
}

struct SyncResult: Decodable, Sendable {
    let pushed: Int
    let pulled: Int
}

/// A pairing ticket from a computer's Settings (or `recall host pair`, or `recall-hub pair`): `rchub1…`, or the
/// `recall://pair/<ticket>` link its QR code holds. Read case-insensitively.
func pairingTicket(_ text: String) -> String? {
    let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
    let last = text.split(separator: "/").last.map(String.init) ?? text
    guard last.lowercased().hasPrefix("rchub1"), last.count > 60,
          last.allSatisfy({ $0.isASCII && ($0.isLetter || $0.isNumber) })
    else { return nil }
    return last.lowercased()
}

private struct Request: Encodable {
    let id = 1
    let method: String
    let params: AnyParams?
}

private struct AnyParams: Encodable {
    let value: any Encodable
    init(_ value: any Encodable) { self.value = value }
    func encode(to encoder: Encoder) throws { try value.encode(to: encoder) }
}

private struct Reply<R: Decodable>: Decodable {
    let result: R?
    let error: CoreError?
}

private struct ID: Encodable, Sendable { let id: String }
private struct DeckRename: Encodable, Sendable { let id: String; let name: String }
private struct DeckFilter: Encodable, Sendable { let deckId: String? }
private struct CardBody: Encodable, Sendable { let id: String?; let content: String; let tags: [String]; let deckId: String }
private struct Answer: Encodable, Sendable { let cardId: String; let remembered: Bool; let durationMs: Int64 }
private struct HubLogin: Encodable, Sendable { let hub: String; let name: String }
private struct CloudLogin: Encodable, Sendable { let server: String; let token: String? }
private struct Paired: Decodable, Sendable { let libraryId: String }
private struct LoggedIn: Decodable, Sendable { let token: String }

private struct CardQuery: Encodable, Sendable {
    let deckId: String
    let q: String?
    let tag: String?
    let limit: Int
    let sort: String
    let desc: Bool?
    let seed: UInt32?
}

/// A new deck, or a move: `parent_id` is always sent, since `null` means the top level.
private struct DeckMove: Encodable, Sendable {
    let id: String?
    let name: String?
    let parentId: String?

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encodeIfPresent(id, forKey: .id)
        try c.encodeIfPresent(name, forKey: .name)
        try c.encode(parentId, forKey: .parentId)
    }

    private enum CodingKeys: String, CodingKey { case id, name, parentId }
}

private let encoder: JSONEncoder = {
    let encoder = JSONEncoder()
    encoder.keyEncodingStrategy = .convertToSnakeCase
    return encoder
}()

private let decoder: JSONDecoder = {
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    return decoder
}()

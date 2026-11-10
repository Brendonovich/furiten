import Foundation

// The contract's types (docs/contract.md), as the HTTP API sends them. Times are Unix milliseconds.

struct DeckSummary: Codable, Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    let parentId: String?
    let createdAt: Int64
    let updatedAt: Int64
    let cards: Int
    let due: Int
    let new: Int

    var toStudy: Int { due + new }
}

struct Deck: Codable, Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    let parentId: String?
}

struct Schedule: Codable, Hashable, Sendable {
    enum State: String, Codable, Sendable { case new, learning, review, relearning }

    let state: State
    let due: Int64
    let stability: Double
    let difficulty: Double
    let reps: Int
    let lapses: Int
    let lastReview: Int64?
}

struct CardView: Codable, Identifiable, Hashable, Sendable {
    let id: String
    let deckId: String
    let content: String
    let tags: [String]
    let createdAt: Int64
    let updatedAt: Int64
    let sides: [String]
    let schedule: Schedule
}

struct Remaining: Codable, Hashable, Sendable {
    let due: Int
    let new: Int

    var total: Int { due + new }
}

struct NextReview: Codable, Sendable {
    let card: CardView?
    let remaining: Remaining
}

struct TagSummary: Codable, Hashable, Sendable {
    let tag: String
    let cards: Int
}

struct Ack: Codable, Sendable {
    let ok: Bool
}

/// A review's answer: whether you remembered the card.
enum Recalled: CaseIterable, Identifiable, Sendable {
    case forgot, remembered

    var id: Self { self }

    var label: String {
        switch self {
        case .forgot: "Forgot"
        case .remembered: "Remembered"
        }
    }
}

/// The deck page's sort (docs/contract.md, "sort"); it never changes the review queue.
enum Sort: String, CaseIterable, Identifiable, Sendable {
    case created, updated, name, due, reviewed, interval, retention, random

    var id: String { rawValue }

    var label: String {
        switch self {
        case .created: "Date created"
        case .updated: "Last updated"
        case .name: "Alphanumeric"
        case .due: "Due date"
        case .reviewed: "Last reviewed"
        case .interval: "Last interval"
        case .retention: "Retention rate"
        case .random: "Random"
        }
    }

    /// The label on the sort control, as short as the web's.
    var short: String {
        switch self {
        case .created: "Created"
        case .updated: "Updated"
        case .name: "Name"
        case .due: "Due"
        case .reviewed: "Reviewed"
        case .interval: "Interval"
        case .retention: "Retention"
        case .random: "Random"
        }
    }

    /// What ascending and descending read as; random has no direction.
    var directions: (String, String)? {
        switch self {
        case .created, .updated, .reviewed: ("Oldest first", "Newest first")
        case .name: ("A to Z", "Z to A")
        case .due: ("Soonest first", "Latest first")
        case .interval: ("Shortest first", "Longest first")
        case .retention: ("Lowest first", "Highest first")
        case .random: nil
        }
    }

    /// Sorts that start newest-first when picked (apps/desktop/src/app.rs).
    var descendingFirst: Bool { self == .updated || self == .reviewed }
}

/// How a deck's cards are laid out.
enum CardLayout: String, CaseIterable, Identifiable, Sendable {
    case grid, list, column

    var id: String { rawValue }

    var label: String {
        switch self {
        case .grid: "Grid view"
        case .list: "List view"
        case .column: "Column view"
        }
    }

    var icon: String {
        switch self {
        case .grid: "square.grid.2x2"
        case .list: "rectangle.grid.1x2"
        case .column: "rectangle.split.2x1"
        }
    }
}

extension Date {
    init(ms: Int64) {
        self.init(timeIntervalSince1970: TimeInterval(ms) / 1000)
    }
}

/// Now, in Unix milliseconds.
func nowMs() -> Int64 {
    Int64(Date.now.timeIntervalSince1970 * 1000)
}

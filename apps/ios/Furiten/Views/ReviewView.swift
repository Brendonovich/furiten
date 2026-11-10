import SwiftUI

/// One review session over a deck and its subdecks (apps/web/src/Review.tsx), or with no deck, the inbox: every due
/// card from every deck. Sides reveal one at a time with a tap anywhere. Swipe right if you remembered it or left if you
/// forgot, anywhere below the toolbar, or use the buttons, at any side (VoiceOver gets the answers as actions on the
/// card). In the inbox on iPhone, a drag from the leading edge or on the buttons pulls the sidebar instead. The queue
/// and scheduling are the desktop's; this only asks for the next card.
struct ReviewView: View {
    @Environment(Library.self) private var library
    @Environment(\.palette) private var palette
    @Environment(\.dismiss) private var dismiss
    @Environment(\.showSidebar) private var showSidebar
    let deckId: String?

    private var inbox: Bool { deckId == nil }

    @State private var next: NextReview?
    @State private var shown = 1
    @State private var answered = 0
    @State private var drag: CGFloat = 0
    @State private var leaving: CGFloat = 0
    @State private var horizontal: Bool?
    @State private var busy = false
    @State private var started = Date.now
    @State private var adding = false

    /// How far a card is dragged before letting go grades it.
    private let swipe: CGFloat = 96

    var body: some View {
        NavigationStack {
            Group {
                if let next {
                    if let card = next.card {
                        session(card)
                    } else {
                        finished
                    }
                } else {
                    ProgressView("Loading…").frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            .background(palette.page.ignoresSafeArea())
            .navigationTitle(deckId.flatMap { id in library.decks.first { $0.id == id }?.name } ?? "Review")
            .navigationSubtitle(next.map { counts($0.remaining.due, $0.remaining.new) } ?? "")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                if inbox, let showSidebar {
                    ToolbarItem(placement: .topBarLeading) {
                        Button("Decks", systemImage: "sidebar.leading", action: showSidebar)
                    }
                }
                if !inbox {
                    ToolbarItem(placement: .topBarLeading) {
                        Button(role: .close, action: close).keyboardShortcut(.cancelAction)
                    }
                }
            }
        }
        .sensoryFeedback(.impact(weight: .light), trigger: answered)
        .sensoryFeedback(.selection, trigger: abs(drag) >= swipe)
        .sheet(isPresented: $adding, onDismiss: { Task { await load() } }) {
            if let deckId { CardEditor(editing: .new(deck: deckId)) }
        }
        // Once the inbox has run out, look again whenever the library changes.
        .task { if next?.card == nil { await load() } }
        .onChange(of: library.refreshed) {
            if inbox, next != nil, next?.card == nil, !busy { Task { await load() } }
        }
    }

    private func revealed(_ card: CardView) -> Bool { shown >= card.sides.count }

    /// The card, top-aligned, as wide as its text needs between 420pt and `maxWidth` (on a phone that's the full
    /// width, as far in from the edges as the answers), with its deck above and the answers along the bottom. A tap
    /// anywhere but the buttons shows the next side; a swipe anywhere above the buttons drags the card.
    private func session(_ card: CardView) -> some View {
        GeometryReader { geo in
            VStack(spacing: 0) {
                ScrollView {
                    // Keyed, so each card (and its deck) comes in afresh rather than only its text changing.
                    ReviewColumn(minWidth: 0) {
                        if inbox || card.deckId != deckId {
                            Label(deckPath(library.decks, card.deckId), systemImage: "book.closed")
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                                .labelIconToTitleSpacing(6)
                                .padding(.leading, 4)
                                .padding(.bottom, 10)
                        }
                        face(card, width: geo.size.width)
                        if !revealed(card) {
                            Text("Tap to reveal the next side")
                                .font(.subheadline)
                                .foregroundStyle(.tertiary)
                                .frame(maxWidth: .infinity)
                                .padding(.top, 20)
                                // Fades as the card is swiped away.
                                .opacity(1 - abs(intent))
                                .layoutValue(key: Stretches.self, value: true)
                        }
                    }
                    .frame(maxWidth: maxWidth)
                    .frame(maxWidth: .infinity)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
                    .id(card.id)
                    .transition(.asymmetric(
                        insertion: .offset(y: 16).combined(with: .scale(scale: 0.98)).combined(with: .opacity),
                        removal: .identity))
                }
                .scrollBounceBehavior(.basedOnSize)
                .simultaneousGesture(swiping)
                .drawerExcluded(inbox)
                answers(card)
            }
            .background {
                // On a hardware keyboard, Space or Return shows the next side, as on the web.
                ForEach([KeyEquivalent.space, .return], id: \.character) { key in
                    Button("Show the next side") { reveal(card) }.keyboardShortcut(key, modifiers: [])
                }
                .opacity(0)
                .accessibilityHidden(true)
            }
            .contentShape(.rect)
            .onTapGesture { reveal(card) }
        }
    }

    @Environment(\.horizontalSizeClass) private var sizeClass
    private var maxWidth: CGFloat { sizeClass == .regular ? 760 : 620 }

    /// How far toward an answer the swipe is, from -1 (forgot) to 1 (remembered).
    private var intent: CGFloat { max(-1, min(1, (leaving != 0 ? leaving * swipe : drag) / swipe)) }

    /// How far toward answering this way the swipe is, from 0 to 1.
    private func toward(_ recalled: Recalled) -> CGFloat { max(0, recalled == .remembered ? intent : -intent) }

    /// How strongly the card's edge shows the answer's colour: it starts a little way in and is full half again past
    /// the point letting go answers, so it keeps building as you commit.
    private var edge: CGFloat {
        leaving != 0 ? 1 : max(0, min(1, (abs(drag) - swipe / 4) / (swipe * 1.25)))
    }

    private func face(_ card: CardView, width: CGFloat) -> some View {
        // Just far enough to be off the screen, so it's gone as the fly-off ends.
        let dx = leaving != 0 ? leaving * width : drag
        let revealed = revealed(card)
        return ReviewColumn(minWidth: 420) {
            ForEach(Array(card.sides.prefix(shown).enumerated()), id: \.offset) { i, side in
                if i > 0 {
                    // Runs the card's full width.
                    Line()
                        .stroke(style: StrokeStyle(lineWidth: 1, dash: [3, 4]))
                        .foregroundStyle(.tertiary)
                        .frame(height: 1)
                        .padding(.vertical, 20)
                        .layoutValue(key: Stretches.self, value: true)
                }
                MarkdownView(text: side)
                    .font(.title2)
                    .lineSpacing(4)
                    .padding(.horizontal, 24)
                    .padding(.top, i == 0 ? 26 : 0)
                    // Slides down into the space the card grows to make for it.
                    .transition(.opacity.combined(with: .offset(y: -8)))
            }
            if revealed, !card.tags.isEmpty {
                FlowLayout { ForEach(card.tags, id: \.self) { TagChip(tag: $0) } }
                    .padding(.horizontal, 24)
                    .padding(.top, 22)
                    .layoutValue(key: Stretches.self, value: true)
                    .transition(.opacity.combined(with: .offset(y: -8)))
            }
            Color.clear.frame(height: 26).layoutValue(key: Stretches.self, value: true)
        }
        // A new side is laid out at its place straight away while the card grows to it, so keep it inside the card.
        .clipShape(.rect(cornerRadius: 24, style: .continuous))
        .cardSurface(radius: 24, raised: true)
        .overlay { SwipeEdge(colour: intent > 0 ? palette.remembered : palette.forgot, strength: edge, radius: 24) }
        .contentShape(.rect)
        .accessibilityElement(children: .combine)
        .accessibilityAction(named: "Remembered") { Task { await answer(.remembered) } }
        .accessibilityAction(named: "Forgot") { Task { await answer(.forgot) } }
        .accessibilityAction(named: "Show the next side") { reveal(card) }
        // Turned where it is, then moved, as the web's `translateX() rotate()`; the other way round it would turn
        // around where it started and swing down as it goes.
        .rotationEffect(.degrees(dx / 24))
        .offset(x: dx)
    }

    /// The card follows the finger, at any side; past `swipe` it flies off and answers.
    private var swiping: some Gesture {
        DragGesture(minimumDistance: 8, coordinateSpace: .global)
            .onChanged { value in
                if horizontal == nil {
                    // In the inbox on iPhone, a drag from the leading edge pulls the sidebar instead.
                    let edge = inbox && showSidebar != nil && value.startLocation.x < drawerEdge
                    horizontal = !edge && abs(value.translation.width) > abs(value.translation.height)
                }
                guard horizontal == true, !busy, next?.card != nil else { return }
                drag = value.translation.width
            }
            .onEnded { value in
                defer { horizontal = nil }
                guard horizontal == true, !busy else { return }
                let flung = abs(value.predictedEndTranslation.width) >= swipe * 3
                if abs(drag) >= swipe || flung {
                    let direction: CGFloat = (abs(drag) >= swipe ? drag : value.predictedEndTranslation.width) > 0 ? 1 : -1
                    Task { await answer(direction > 0 ? .remembered : .forgot) }
                } else {
                    withAnimation(ReviewCurve.settle) { drag = 0 }
                }
            }
    }

    /// Forgot and Remembered as icons on glass, at any side. A swipe presses its answer's button and fades the other.
    private func answers(_ card: CardView) -> some View {
        HStack(spacing: 8) {
            ForEach([Recalled.forgot, .remembered], id: \.self) { recalled in
                Button(recalled.label, systemImage: recalled == .remembered ? "checkmark" : "xmark") {
                    Task { await answer(recalled) }
                }
                .buttonStyle(AnswerButtonStyle(
                    colour: recalled == .remembered ? palette.remembered : palette.forgot,
                    press: toward(recalled), dim: toward(recalled == .remembered ? .forgot : .remembered)))
                .disabled(busy)
                .keyboardShortcut(recalled == .remembered ? "2" : "1", modifiers: [])
            }
        }
        .frame(maxWidth: maxWidth)
        .padding(.horizontal, 16)
        .padding(.top, 12)
        .padding(.bottom, 8)
    }

    @ViewBuilder
    private var finished: some View {
        if inbox { caughtUp } else { deckFinished }
    }

    private var caughtUp: some View {
        ContentUnavailableView {
            Label(answered > 0 ? "That’s everything for now" : "All caught up", systemImage: "checkmark.circle")
        } description: {
            Text("Cards from every deck show up here when they’re due.")
        }
    }

    private var deckFinished: some View {
        let due = library.cards.filter { $0.schedule.state != .new }.map(\.schedule.due).min()
        return ContentUnavailableView {
            Label(answered > 0 ? "That’s everything for now" : "Nothing to review", systemImage: "checkmark.circle")
        } description: {
            Text(due.map { "The next card is due \(until($0))." } ?? "Add cards and they’ll show up here.")
        } actions: {
            HStack(spacing: 12) {
                Button("Add cards") { adding = true }.buttonStyle(.glass)
                Button("Done", action: close).buttonStyle(.glassProminent)
            }
            .controlSize(.large)
        }
    }

    private func reveal(_ card: CardView) {
        guard shown < card.sides.count else { return }
        // The card grows to fit the new side rather than jumping to its new height.
        withAnimation(ReviewCurve.reveal) { shown += 1 }
    }

    private func close() {
        dismiss()
        Task { await library.refresh() }
    }

    private func load() async {
        guard let core = library.core else { return }
        do {
            let next = try await core.next(deck: deckId)
            withAnimation(ReviewCurve.cardIn) {
                self.next = next
                shown = 1
                #if DEBUG
                if Scripted.reveal { shown = next.card?.sides.count ?? 1 }
                #endif
                drag = 0
                leaving = 0
            }
            started = .now
            if next.card == nil, !inbox { await library.refresh() }
        } catch {
            library.fail(error)
        }
    }

    private func answer(_ recalled: Recalled) async {
        guard let card = next?.card, let core = library.core, !busy else { return }
        busy = true
        defer { busy = false }
        // The card flies off the way a swipe would send it, and the next one comes in once it's gone.
        withAnimation(ReviewCurve.flyOff) { leaving = recalled == .remembered ? 1 : -1 }
        do {
            async let gone: Void? = try? Task.sleep(for: .seconds(ReviewCurve.flyOffDuration))
            _ = try await core.answer(card.id, recalled, durationMs: Int64(Date.now.timeIntervalSince(started) * 1000))
            _ = await gone
            answered += 1
            await load()
            // Keep the sidebar's counts current.
            if inbox { await library.refreshDecks() }
        } catch {
            library.fail(error)
            withAnimation(ReviewCurve.settle) {
                leaving = 0
                drag = 0
            }
        }
    }
}

/// The answer's colour on the card's edge as it's dragged: a thin line with a glow either side of it.
private struct SwipeEdge: View {
    let colour: Color
    let strength: CGFloat
    let radius: CGFloat

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: radius, style: .continuous)
        ZStack {
            shape.stroke(colour.opacity(0.3 * strength), lineWidth: 20).blur(radius: 10).clipShape(shape)
            shape.strokeBorder(colour.opacity(strength), lineWidth: 1)
                .shadow(color: colour.opacity(0.5 * strength), radius: 4)
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

/// An answer button: the icon on interactive glass, taking the answer's colour while pressed. `press` (how far a
/// swipe is toward this answer) fades in the pressed look; `dim` (toward the other) fades the button out.
private struct AnswerButtonStyle: ButtonStyle {
    let colour: Color
    var press: CGFloat = 0
    var dim: CGFloat = 0

    func makeBody(configuration: Configuration) -> some View {
        let pressed = configuration.isPressed ? 1 : press
        configuration.label
            .labelStyle(.iconOnly)
            .font(.title3.weight(.medium))
            .foregroundStyle(Color.primary.mix(with: colour, by: pressed))
            .frame(maxWidth: .infinity, minHeight: 54)
            .contentShape(.capsule)
            .glassEffect(.regular.interactive(), in: .capsule)
            .scaleEffect(1 - 0.03 * pressed)
            .opacity(1 - 0.6 * dim)
            .animation(.easeOut(duration: 0.12), value: configuration.isPressed)
            // As on the web, the buttons follow a swipe on their own quicker curve, not the card's.
            .animation(ReviewCurve.button, value: press)
            .animation(ReviewCurve.button, value: dim)
    }
}

/// Marks a view in a `ReviewColumn` that takes whatever width the column has rather than setting it: rules, hints,
/// tags.
private struct Stretches: LayoutValueKey {
    static let defaultValue = false
}

/// A top-down stack as wide as its widest fixed child (at least `minWidth`, at most what's offered), the way a web
/// `fit-content` box sizes. Every child is then offered that width, on the leading edge.
private struct ReviewColumn: Layout {
    var minWidth: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = width(proposal, subviews)
        let height = subviews.reduce(0) { $0 + $1.sizeThatFits(ProposedViewSize(width: width, height: nil)).height }
        return CGSize(width: width, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var y = bounds.minY
        let offer = ProposedViewSize(width: bounds.width, height: nil)
        for view in subviews {
            let size = view.sizeThatFits(offer)
            view.place(at: CGPoint(x: bounds.minX, y: y), proposal: ProposedViewSize(width: bounds.width, height: size.height))
            y += size.height
        }
    }

    private func width(_ proposal: ProposedViewSize, _ subviews: Subviews) -> CGFloat {
        let available = proposal.width ?? .infinity
        let offer = ProposedViewSize(width: available.isFinite ? available : nil, height: nil)
        let natural = subviews.filter { !$0[Stretches.self] }.map { $0.sizeThatFits(offer).width }.max() ?? 0
        let width = max(natural, min(minWidth, available))
        return available.isFinite ? min(width, available) : width
    }
}

/// The review's animation curves, the web's own (apps/web/src/styles.css, `.review-card`), so a card moves the same
/// in both apps.
private enum ReviewCurve {
    /// An answered card goes fast and slows as it reaches the edge, and is off the screen as it ends
    /// (`.review-card.leaving`, `FLY_OFF` in Review.tsx).
    static let flyOffDuration = 0.22
    static let flyOff = Animation.timingCurve(0.25, 0.6, 0.4, 1, duration: flyOffDuration)
    /// A card let go short of answering returns to the middle (`.review-card.settling`).
    static let settle = Animation.timingCurve(0.2, 0.8, 0.2, 1, duration: 0.3)
    /// The next card rises in (`@keyframes card-in`).
    static let cardIn = Animation.timingCurve(0.2, 0.8, 0.2, 1, duration: 0.32)
    /// The next side slides down into view (`.reveal-in`).
    static let reveal = Animation.timingCurve(0.2, 0.8, 0.2, 1, duration: 0.28)
    /// The answer buttons pressing and fading with a swipe: CSS's `ease` (`.answer`).
    static let button = Animation.timingCurve(0.25, 0.1, 0.25, 1, duration: 0.12)
}

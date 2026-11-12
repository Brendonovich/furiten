import SwiftUI

/// What the sidebar has open: the inbox (every due card from every deck), a deck, or every card with a tag.
enum SidebarItem: Hashable {
    case review
    case deck(String)
    case tag(String)
}

extension EnvironmentValues {
    /// Opens the sidebar drawer, on iPhone; nil where the sidebar is always there (iPad's split view shows its own
    /// toggle).
    @Entry var showSidebar: (() -> Void)?
}

/// The sidebar: Review at the top, then the decks. On iPhone a drawer that slides in from the leading edge over the
/// inbox or a deck (the inbox to start with), as OpenCode's app does; on iPad a split view. Settings open as a sheet;
/// search, through every deck's cards, as a layer over whatever's open, which closing it leaves as it was.
struct Shell: View {
    @Environment(Library.self) private var library
    @Environment(\.horizontalSizeClass) private var size
    @Environment(\.palette) private var palette
    /// What's on screen. Unlike `library.deckId` (the last deck opened, which the library keeps), it's nil while
    /// iPad's split view has nothing chosen.
    @State private var selection: SidebarItem? = .review
    @State private var drawer = false
    @State private var path = NavigationPath()
    @State private var settings = false
    @State private var searching = false

    var body: some View {
        Group {
            if size == .compact {
                Drawer(open: $drawer, edgeSwipe: path.isEmpty) {
                    NavigationStack {
                        DeckList(
                            selection: Binding(get: { selection }, set: { pick($0) }), settings: $settings,
                            search: { search() }, drawer: true
                        )
                        .listBackground(palette.sidebar)
                    }
                } content: {
                    detail.environment(\.showSidebar, { withAnimation(drawerSpring) { drawer = true } })
                }
            } else {
                NavigationSplitView {
                    DeckList(selection: $selection, settings: $settings, search: { search() })
                } detail: {
                    detail
                }
            }
        }
        .sheet(isPresented: $settings) { SettingsView() }
        #if DEBUG
        .onAppear {
            switch Scripted.screen {
            case nil, "inbox": break
            case "decks": drawer = true
            case "settings": settings = true
            case "search": searching = true
            case "tag": selection = Scripted.tag.map(SidebarItem.tag)
            default: selection = library.deckId.map(SidebarItem.deck)
            }
        }
        #endif
        .onChange(of: selection) { _, item in
            path = NavigationPath()
            if searching { withAnimation(searchAnimation) { searching = false } }
            if case .deck(let id) = item, id != library.deckId { library.selectDeck(id) }
        }
        // The library moved to another deck (one just made, or after the open one was deleted): follow it.
        .onChange(of: library.deckId) { _, id in
            if case .deck = selection { selection = id.map(SidebarItem.deck) ?? .review }
        }
    }

    /// What's open, with the search over it while it's open.
    private var detail: some View {
        ZStack {
            page
                .accessibilityHidden(searching)
                // Under the search, the page's own drags (the review's card) don't hold the drawer back.
                .transformPreference(DrawerExclusion.self) { if searching { $0 = [] } }
            if searching {
                NavigationStack {
                    SearchScreen { withAnimation(searchAnimation) { searching = false } }
                }
                .transition(.opacity.combined(with: .offset(y: 12)))
                .zIndex(1)
            }
        }
    }

    @ViewBuilder
    private var page: some View {
        switch selection {
        case .review:
            ReviewView(deckId: nil)
        case .deck:
            NavigationStack(path: $path) { DeckScreen() }
        case .tag(let tag):
            NavigationStack(path: $path) { TagScreen(tag: tag) }.id(tag)
        case nil:
            NavigationStack {
                ContentUnavailableView("Choose a deck", systemImage: "book.closed", description: Text("Its cards show up here."))
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .listBackground(palette.list)
            }
        }
    }

    /// Picking anything in the drawer, even what's already open, closes it, and the search.
    private func pick(_ item: SidebarItem?) {
        if let item { selection = item }
        withAnimation(searchAnimation) { searching = false }
        withAnimation(drawerSpring) { drawer = false }
    }

    /// Opens the search over what's open, closing the drawer.
    private func search() {
        withAnimation(searchAnimation) { searching = true }
        withAnimation(drawerSpring) { drawer = false }
    }
}

private let drawerSpring = Animation.spring(duration: 0.22, bounce: 0)
private let searchAnimation = Animation.smooth(duration: 0.25)

/// A sidebar under the page: the page slides aside, dimming, to show it, from the toolbar's button or a drag to the right
/// anywhere on the page, and slides back with a tap on what's left of the page or a drag to the left anywhere. A drag
/// that starts on a sideways scroller, or over a `drawerExcluded()` area (the review's card, which swipes to answer)
/// past the leading edge, is left to them.
private struct Drawer<Sidebar: View, Content: View>: View {
    @Binding var open: Bool
    /// Off while the page has something pushed, so a drag from the edge goes back instead.
    let edgeSwipe: Bool
    @ViewBuilder let sidebar: Sidebar
    @ViewBuilder let content: Content

    @State private var drag: CGFloat?
    @State private var excluded: [CGRect] = []

    var body: some View {
        GeometryReader { geo in
            let width = min(290, (geo.size.width * 0.74).rounded())
            let x = max(0, min(width, (open ? width : 0) + (drag ?? 0)))
            let progress = x / width
            ZStack(alignment: .leading) {
                sidebar
                    .frame(width: width)
                    .offset(x: (x - width) / 3)
                    .allowsHitTesting(open && drag == nil)
                content
                    .onPreferenceChange(DrawerExclusion.self) { excluded = $0 }
                    .frame(width: geo.size.width)
                    .allowsHitTesting(!open && drag == nil)
                    .overlay {
                        if progress > 0 {
                            Color.black.opacity(0.18 * progress)
                                .ignoresSafeArea()
                                .contentShape(.rect)
                                .onTapGesture { settle(open: false) }
                        }
                    }
                    .shadow(color: .black.opacity(0.16 * progress), radius: 20)
                    .offset(x: x)
            }
            .contentShape(.rect)
            .gesture(DrawerPan(
                shouldBegin: { start, dx in
                    if open { return dx < 0 }
                    guard dx > 0, edgeSwipe else { return false }
                    return start.x < drawerEdge || !excluded.contains { $0.contains(start) }
                },
                changed: { drag = $0 },
                ended: { dx, velocity in
                    // A flick decides it; otherwise it's how far it got, a third of the way from where it started.
                    let from: CGFloat = open ? width : 0
                    let at = max(0, min(width, from + dx))
                    settle(open: velocity > 400 || (velocity > -400 && at > width * (open ? 2 / 3 : 1 / 3)))
                }))
        }
        .sensoryFeedback(.impact(weight: .light), trigger: open)
    }

    private func settle(open: Bool) {
        withAnimation(drawerSpring) {
            self.open = open
            drag = nil
        }
    }
}

/// How far in from the leading edge a drag pulls the sidebar even over a `drawerExcluded()` area.
let drawerEdge: CGFloat = 28

/// Areas, on screen, where a drag to the right is the page's own rather than the sidebar's.
private struct DrawerExclusion: PreferenceKey {
    static let defaultValue: [CGRect] = []
    static func reduce(value: inout [CGRect], nextValue: () -> [CGRect]) { value += nextValue() }
}

extension View {
    /// Leaves drags to the right here to this view, but for ones from the leading edge, which pull the sidebar.
    func drawerExcluded(_ excluded: Bool = true) -> some View {
        background {
            GeometryReader { geo in
                Color.clear.preference(key: DrawerExclusion.self, value: excluded ? [geo.frame(in: .global)] : [])
            }
        }
    }
}

/// The drawer's drag, as a UIKit pan so it's measured on screen (the page moves under the finger), can decide when it
/// starts whether it's the sidebar's, and wins over scroll views and buttons once it has.
private struct DrawerPan: UIGestureRecognizerRepresentable {
    /// Where the drag started, on screen, and which way it's going; asked once it's clearly sideways.
    let shouldBegin: (CGPoint, CGFloat) -> Bool
    let changed: (CGFloat) -> Void
    /// How far it went and how fast it was going, sideways, when let go.
    let ended: (CGFloat, CGFloat) -> Void

    func makeCoordinator(converter: CoordinateSpaceConverter) -> Coordinator { Coordinator() }

    func makeUIGestureRecognizer(context: Context) -> UIPanGestureRecognizer {
        let pan = UIPanGestureRecognizer()
        pan.maximumNumberOfTouches = 1
        pan.delegate = context.coordinator
        return pan
    }

    func updateUIGestureRecognizer(_ pan: UIPanGestureRecognizer, context: Context) {
        context.coordinator.shouldBegin = shouldBegin
    }

    func handleUIGestureRecognizerAction(_ pan: UIPanGestureRecognizer, context: Context) {
        let dx = pan.translation(in: nil).x
        switch pan.state {
        case .began, .changed: changed(dx)
        case .ended: ended(dx, pan.velocity(in: nil).x)
        case .cancelled, .failed: ended(0, 0)
        default: break
        }
    }

    final class Coordinator: NSObject, UIGestureRecognizerDelegate {
        var shouldBegin: (CGPoint, CGFloat) -> Bool = { _, _ in false }

        func gestureRecognizerShouldBegin(_ g: UIGestureRecognizer) -> Bool {
            guard let pan = g as? UIPanGestureRecognizer else { return false }
            let t = pan.translation(in: nil)
            guard abs(t.x) > abs(t.y) * 1.4 else { return false }
            let start = CGPoint(x: pan.location(in: nil).x - t.x, y: pan.location(in: nil).y - t.y)
            if let view = g.view, scrollsSideways(view.hitTest(pan.location(in: view), with: nil)) { return false }
            return shouldBegin(start, t.x)
        }

        /// Text, and anything that scrolls sideways (code blocks, tag rows), keeps its own drags.
        private func scrollsSideways(_ view: UIView?) -> Bool {
            var at = view
            while let v = at {
                if v is UITextView || v is UITextField { return true }
                if let s = v as? UIScrollView, s.contentSize.width > s.bounds.width + 1 { return true }
                at = v.superview
            }
            return false
        }

        // Scroll views wait to see whether the drag is the sidebar's, so a sideways drag doesn't also scroll them.
        func gestureRecognizer(_ g: UIGestureRecognizer, shouldBeRequiredToFailBy other: UIGestureRecognizer) -> Bool {
            other.view is UIScrollView
        }

        // The page's own gestures (taps, the review's swipe) run alongside; once it's the sidebar's, the page stops
        // taking touches (`Drawer` turns its hit testing off), so a drag over a button doesn't press it.
        func gestureRecognizer(_ g: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
            !(other.view is UIScrollView)
        }
    }
}

private struct DeckList: View {
    @Environment(Library.self) private var library
    @Environment(\.palette) private var palette
    @Environment(\.defaultMinListRowHeight) private var systemRowHeight
    @Binding var selection: SidebarItem?
    @Binding var settings: Bool
    /// Opens the search over what's open.
    let search: () -> Void
    /// In iPhone's drawer, which has no navigation bar, so the list starts right under the status bar; iPad's split view
    /// keeps its bar for the sidebar toggle.
    var drawer = false
    @State private var naming = false
    @State private var name = ""
    var body: some View {
        List(selection: $selection) {
            Section {
                HStack {
                    Label("Review", systemImage: "tray.full")
                    Spacer(minLength: 8)
                    let count = inboxCount(library.decks)
                    if count > 0 { Text("\(count)").font(.subheadline.monospacedDigit()).foregroundStyle(palette.accent) }
                }
                .sidebarRow(.review, selection: selection, drawer: drawer)
            } header: {
                if drawer {
                    Text("Furiten")
                        .font(.title2.bold())
                        .foregroundStyle(Color.primary)
                        .textCase(nil)
                        .padding(.bottom, 4)
                }
            }
            Section {
                ForEach(deckTree(library.decks), id: \.deck.id) { row in
                    DeckRow(deck: row.deck, depth: row.depth)
                        .sidebarRow(.deck(row.deck.id), selection: selection, drawer: drawer)
                }
            } header: {
                Text("Decks").sidebarHeader(drawer)
            }
            if !library.tags.isEmpty {
                Section {
                    ForEach(library.tags.sorted { $0.tag.localizedStandardCompare($1.tag) == .orderedAscending }, id: \.tag) {
                        TagRow(tag: $0).sidebarRow(.tag($0.tag), selection: selection, drawer: drawer)
                    }
                } header: {
                    Text("Tags").sidebarHeader(drawer)
                }
            }
        }
        .listStyle(.sidebar)
        .listSectionSpacing(drawer ? .custom(4) : .compact)
        .environment(\.defaultMinListRowHeight, drawer ? 44 : systemRowHeight)
        .contentMargins(.top, drawer ? 4 : nil, for: .scrollContent)
        .contentMargins(.horizontal, drawer ? 10 : nil, for: .scrollContent)
        .overlay {
            if library.loaded && library.decks.isEmpty {
                ContentUnavailableView {
                    Label("No decks yet", systemImage: "book.closed")
                } description: {
                    Text("Make a deck to start adding cards.")
                } actions: {
                    Button("New deck", action: newDeck).buttonStyle(.glassProminent)
                }
            } else if !library.loaded {
                ProgressView()
            }
        }
        .refreshable { await library.refresh() }
        .navigationTitle("Furiten")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(removing: .title)
        .toolbar(drawer ? .hidden : .automatic, for: .navigationBar)
        .toolbar {
            if !drawer {
                ToolbarItem(placement: .topBarLeading) {
                    Text("Furiten").font(.title2.bold()).fixedSize().padding(.leading, 8)
                }
                .sharedBackgroundVisibility(.hidden)
            }
            ToolbarItem(placement: .bottomBar) { settingsButton }
            ToolbarSpacer(.flexible, placement: .bottomBar)
            ToolbarItem(placement: .bottomBar) {
                Button("Search", systemImage: "magnifyingglass", action: search)
            }
            ToolbarSpacer(.fixed, placement: .bottomBar)
            ToolbarItem(placement: .bottomBar) {
                Button("New deck", systemImage: "folder.badge.plus", action: newDeck)
            }
        }
        .alert("New deck", isPresented: $naming) {
            TextField("Name", text: $name)
            Button("Cancel", role: .cancel) {}
            Button("Create") {
                let value = name.trimmingCharacters(in: .whitespaces)
                guard !value.isEmpty else { return }
                Task {
                    if let deck = await library.write({ try await $0.createDeck(name: value, parent: nil) }) {
                        selection = .deck(deck.id)
                    }
                }
            }
        }
    }

    /// Settings in the bottom bar, with the sync dot on its corner.
    private var settingsButton: some View {
        Button { settings = true } label: {
            Image(systemName: "gearshape")
                .overlay(alignment: .topTrailing) { SyncDot(sync: library.sync).offset(x: 3, y: -2) }
        }
        .accessibilityLabel("Settings")
    }

    private func newDeck() {
        name = ""
        naming = true
    }
}

/// A green dot, on Settings, while syncing is connected: open to the paired computer or hub, or signed in to Furiten
/// Cloud and syncing without error. Nothing otherwise, since working offline is fine; Settings says more.
private struct SyncDot: View {
    let sync: CoreStatus?

    private var connected: Bool {
        guard let sync, sync.signedIn, sync.syncError == nil else { return false }
        return sync.paired ? sync.connected == true : sync.lastSync != nil
    }

    var body: some View {
        if connected {
            Circle().fill(.green).frame(width: 7, height: 7).accessibilityLabel("Syncing")
        }
    }
}

private struct DeckRow: View {
    @Environment(\.palette) private var palette
    let deck: DeckSummary
    let depth: Int

    var body: some View {
        HStack {
            Label {
                Text(deck.name).lineLimit(1)
            } icon: {
                Image(systemName: depth > 0 ? "arrow.turn.down.right" : "book.closed").foregroundStyle(palette.icon)
            }
            Spacer(minLength: 8)
            HStack(spacing: 8) {
                if deck.due > 0 { Text("\(deck.due)").foregroundStyle(palette.accent) }
                if deck.new > 0 { Text("\(deck.new)").foregroundStyle(.secondary) }
            }
            .font(.subheadline.monospacedDigit())
        }
        .padding(.leading, CGFloat(depth) * 22)
    }
}

extension View {
    /// A row of the sidebar that opens `item`. In iPhone's drawer, where the sidebar list style still draws inset
    /// grouped cards, it's drawn as iPad's sidebar is instead: no card or separators, a capsule behind the open row.
    fileprivate func sidebarRow(_ item: SidebarItem, selection: SidebarItem?, drawer: Bool) -> some View {
        tag(item).modifier(SidebarRow(selected: item == selection, drawer: drawer))
    }
}

private struct SidebarRow: ViewModifier {
    let selected: Bool
    let drawer: Bool

    func body(content: Content) -> some View {
        if drawer {
            content
                .labelStyle(DrawerLabel())
                .frame(minHeight: 44)
                .listRowInsets(EdgeInsets(top: 0, leading: 14, bottom: 0, trailing: 14))
                .listRowBackground(Capsule().fill(selected ? AnyShapeStyle(.fill.secondary) : AnyShapeStyle(.clear)))
                .listRowSeparator(.hidden)
        } else {
            content
        }
    }
}

/// A drawer row's icon and name, closer together than the list's own spacing.
private struct DrawerLabel: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 12) {
            configuration.icon.frame(width: 24)
            configuration.title
        }
    }
}

extension View {
    /// A section's title in the drawer, small, as iPad's sidebar has them.
    fileprivate func sidebarHeader(_ drawer: Bool) -> some View {
        font(drawer ? .subheadline.weight(.semibold) : nil)
    }
}

/// A tag in the sidebar, styled as a deck's row, with how many cards carry it.
private struct TagRow: View {
    @Environment(\.palette) private var palette
    let tag: TagSummary

    var body: some View {
        HStack {
            Label {
                Text(tag.tag).lineLimit(1)
            } icon: {
                Image(systemName: "number").foregroundStyle(palette.icon)
            }
            Spacer(minLength: 8)
            Text("\(tag.cards)")
                .font(.subheadline.monospacedDigit())
                .foregroundStyle(.secondary)
        }
    }
}

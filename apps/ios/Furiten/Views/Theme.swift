import SwiftUI

/// How the app looks, chosen in Settings. System is the system's own look, light or dark as the phone is. Furiten is the
/// web app's palette (always dark, OpenCode's greys, its answer colours) under the same system accent and Liquid Glass.
enum Theme: String, CaseIterable, Identifiable {
    case system
    case furiten

    /// Where the choice is kept, in `UserDefaults`.
    static let key = "theme"

    var id: Self { self }

    var name: String {
        switch self {
        case .system: "System"
        case .furiten: "Furiten"
        }
    }

    var scheme: ColorScheme? { self == .furiten ? .dark : nil }

    var palette: Palette { self == .furiten ? .furiten : .system }
}

/// The colours the screens paint with. Lists and forms keep the system's own backgrounds where these are nil.
struct Palette {
    /// Behind a screen's content.
    let page: Color
    /// Cards, and the sides being edited.
    let card: Color
    /// Code blocks on a card.
    let inset: Color
    /// A quote's bar.
    let rule: Color
    let forgot: Color
    let remembered: Color
    /// Behind a list, a form or an empty screen, where the system's isn't wanted.
    let list: Color?
    /// Behind the sidebar's list, a shade below the page (the web app's drawer).
    let sidebar: Color?
    /// Behind a list or form's rows.
    let row: Color?

    static let system = Palette(
        page: Color(.systemGroupedBackground), card: Color(.secondarySystemGroupedBackground), inset: Color(.tertiarySystemFill),
        rule: Color(.separator), forgot: .red, remembered: .green, list: nil, sidebar: nil, row: nil)

    /// The web app's colours (apps/web/src/styles.css, from OpenCode's dark theme): the page is `--page`, cards and rows
    /// `--layer-1`, code `--layer-2`, the sidebar `--surface`.
    static let furiten = Palette(
        page: Color(hex: 0x161616), card: Color(hex: 0x242424), inset: Color(hex: 0x2E2E2E), rule: .white.opacity(0.1),
        forgot: Color(hex: 0xF17471), remembered: Color(hex: 0x6BD586), list: Color(hex: 0x161616),
        sidebar: Color(hex: 0x080808), row: Color(hex: 0x242424))
}

extension EnvironmentValues {
    @Entry var palette = Palette.system
}

extension View {
    /// The theme's background behind a list, a form or an empty screen, in place of the system's; nil keeps the system's.
    func listBackground(_ color: Color?) -> some View {
        modifier(ListBackground(color: color))
    }
}

private struct ListBackground: ViewModifier {
    let color: Color?

    func body(content: Content) -> some View {
        if let color {
            content.scrollContentBackground(.hidden).background(color)
        } else {
            content
        }
    }
}

private extension Color {
    init(hex: UInt32) {
        self.init(
            red: Double((hex >> 16) & 0xFF) / 255, green: Double((hex >> 8) & 0xFF) / 255, blue: Double(hex & 0xFF) / 255)
    }
}

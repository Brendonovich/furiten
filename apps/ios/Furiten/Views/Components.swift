import SwiftUI

// Small pieces the screens share. The app uses the system's own look: semantic colours (or the Furiten theme's
// palette, see Theme.swift), text styles and Liquid Glass for controls. Content (cards, sides) sits on opaque grouped surfaces with a lit edge; glass is only for controls and
// navigation.

/// A card's surface, with the lit edge the web app's cards have (its `--edge-raised`): a hairline ring, brighter
/// along the top as if lit from above, and a soft shadow.
struct CardSurface: ViewModifier {
    @Environment(\.colorScheme) private var scheme
    @Environment(\.palette) private var palette
    var radius: CGFloat = 16
    var raised = false

    func body(content: Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: radius, style: .continuous)
        let dark = scheme == .dark
        content
            .background(palette.card, in: shape)
            .overlay {
                shape.strokeBorder(
                    LinearGradient(
                        stops: dark
                            ? [.init(color: .white.opacity(0.26), location: 0), .init(color: .white.opacity(0.09), location: 0.35),
                               .init(color: .white.opacity(0.06), location: 1)]
                            : [.init(color: .black.opacity(0.05), location: 0), .init(color: .black.opacity(0.09), location: 1)],
                        startPoint: .top, endPoint: .bottom),
                    lineWidth: dark ? 0.75 : 0.5
                )
            }
            .shadow(color: .black.opacity(dark ? 0.3 : raised ? 0.08 : 0.04), radius: raised ? 16 : 2, y: raised ? 8 : 1)
    }
}

extension View {
    func cardSurface(radius: CGFloat = 16, raised: Bool = false) -> some View {
        modifier(CardSurface(radius: radius, raised: raised))
    }
}

/// "New", "Due" (tinted), or how long until it's due, in the surrounding style otherwise.
struct DueLabel: View {
    let card: CardView

    var body: some View {
        if card.schedule.state == .new {
            Text("New")
        } else if card.schedule.due <= nowMs() {
            Text("Due").foregroundStyle(.tint).fontWeight(.semibold)
        } else {
            Text(untilShort(card.schedule.due))
        }
    }
}

/// A card's tags as small tinted capsules.
struct TagChip: View {
    let tag: String

    var body: some View {
        Text("#\(tag)")
            .font(.footnote)
            .foregroundStyle(.tint)
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .background(.tint.quaternary, in: .capsule)
    }
}

/// A pressed tile shrinks a little.
struct TileButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? 0.97 : 1)
            .animation(.easeOut(duration: 0.12), value: configuration.isPressed)
    }
}

/// Tags laid out in rows, wrapping like text.
struct FlowLayout: Layout {
    var spacing: CGFloat = 6

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let rows = rows(proposal.width ?? .infinity, subviews)
        let height = rows.reduce(0) { $0 + $1.height } + spacing * CGFloat(max(0, rows.count - 1))
        let width = rows.map(\.width).max() ?? 0
        return CGSize(width: proposal.width ?? width, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var y = bounds.minY
        for row in rows(bounds.width, subviews) {
            var x = bounds.minX
            for i in row.items {
                let size = subviews[i].sizeThatFits(.unspecified)
                let width = min(size.width, bounds.width)
                subviews[i].place(
                    at: CGPoint(x: x, y: y + (row.height - size.height) / 2), proposal: ProposedViewSize(width: width, height: size.height))
                x += width + spacing
            }
            y += row.height + spacing
        }
    }

    private func rows(_ maxWidth: CGFloat, _ subviews: Subviews) -> [(items: [Int], width: CGFloat, height: CGFloat)] {
        var rows: [(items: [Int], width: CGFloat, height: CGFloat)] = []
        var current: (items: [Int], width: CGFloat, height: CGFloat) = ([], 0, 0)
        for (i, view) in subviews.enumerated() {
            let size = view.sizeThatFits(.unspecified)
            let width = min(size.width, maxWidth)
            if !current.items.isEmpty && current.width + spacing + width > maxWidth {
                rows.append(current)
                current = ([], 0, 0)
            }
            current.width += (current.items.isEmpty ? 0 : spacing) + width
            current.height = max(current.height, size.height)
            current.items.append(i)
        }
        if !current.items.isEmpty { rows.append(current) }
        return rows
    }
}

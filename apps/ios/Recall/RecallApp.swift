import SwiftUI

@main
struct RecallApp: App {
    @Environment(\.scenePhase) private var phase
    @State private var library = Library()
    /// A pairing code from a `recall://pair/…` link (the computer's QR code, scanned with the Camera app).
    @State private var link: Link?

    var body: some Scene {
        WindowGroup {
            Shell()
                .overlay(alignment: .bottom) { Toast() }
                .onOpenURL { url in
                    if let ticket = pairingTicket(url.absoluteString) { link = Link(ticket: ticket) }
                }
                .sheet(item: $link) { link in PairView(ticket: link.ticket) }
                .onChange(of: phase) { _, phase in
                    switch phase {
                    case .active: library.resume()
                    case .background: library.suspend()
                    default: break
                    }
                }
                .environment(library)
        }
    }

    private struct Link: Identifiable {
        let ticket: String
        var id: String { ticket }
    }
}

/// A notice from the last thing that went wrong; tap it to dismiss, or it goes on its own.
private struct Toast: View {
    @Environment(Library.self) private var library

    var body: some View {
        if let notice = library.notice {
            Text(notice)
                .font(.subheadline)
                .padding(.horizontal, 18)
                .padding(.vertical, 12)
                .frame(maxWidth: 480, alignment: .leading)
                .glassEffect(.regular, in: .rect(cornerRadius: 22))
                .padding(.horizontal, 16)
                .padding(.bottom, 76)
                .onTapGesture { library.notice = nil }
                .task(id: notice) {
                    try? await Task.sleep(for: .seconds(4))
                    if !Task.isCancelled, library.notice == notice { library.notice = nil }
                }
                .transition(.move(edge: .bottom).combined(with: .opacity))
        }
    }
}

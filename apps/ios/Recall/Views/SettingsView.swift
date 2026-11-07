import SwiftUI
import VisionKit

/// Settings: where this phone's library syncs. The library lives on the phone either way; pairing with your computer
/// (or signing in to Recall Cloud) keeps it in step with your other devices.
struct SettingsView: View {
    @Environment(Library.self) private var library
    @Environment(\.dismiss) private var dismiss
    @State private var pairing = false
    @State private var confirmDisconnect = false

    var body: some View {
        NavigationStack {
            Form {
                if let sync = library.sync, sync.signedIn {
                    syncing(sync)
                } else {
                    notSyncing
                }
                Section {
                    LabeledContent("Version", value: Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "")
                }
            }
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { Button(role: .close) { dismiss() } }
            }
            .sheet(isPresented: $pairing) { PairView() }
            .confirmationDialog(
                library.sync?.paired == true ? "Stop syncing with your computer?" : "Sign out of Recall Cloud?",
                isPresented: $confirmDisconnect, titleVisibility: .visible
            ) {
                Button(library.sync?.paired == true ? "Disconnect" : "Sign out", role: .destructive) {
                    Task { await library.signOut() }
                }
            } message: {
                Text("Your decks stay on this phone. Pair again to sync them.")
            }
        }
    }

    private var notSyncing: some View {
        Group {
            Section {
                VStack(alignment: .leading, spacing: 12) {
                    Text("Your decks live on this phone, so reviewing and editing work anywhere, even offline. Pair it with Recall on your computer to keep the two in step: they sync whenever both are online, on any network.")
                        .foregroundStyle(.secondary)
                    Button { pairing = true } label: {
                        Label("Pair with your computer", systemImage: "laptopcomputer.and.iphone")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.glassProminent)
                    .controlSize(.large)
                }
                .padding(.vertical, 6)
            } header: {
                Text("Sync")
            }
            Section {
                NavigationLink("Recall Cloud") { CloudSignIn() }
            } footer: {
                Text("Or sync through a Recall Cloud account instead of your computer.")
            }
        }
    }

    private func syncing(_ sync: CoreStatus) -> some View {
        Section {
            if sync.paired {
                LabeledContent("Your computer") {
                    HStack(spacing: 6) {
                        Circle().fill(sync.connected == true ? .green : .orange).frame(width: 7, height: 7)
                        Text(sync.connected == true ? "Connected" : "Not connected")
                    }
                }
            } else {
                LabeledContent("Recall Cloud", value: sync.server.flatMap { URLComponents(string: $0)?.host } ?? "")
            }
            LabeledContent("Last synced", value: sync.lastSync.map { ago(Date(ms: $0)) } ?? "Not synced yet")
            if let error = sync.syncError {
                Text(error).font(.footnote).foregroundStyle(.red)
            }
            Button("Sync now") { Task { await library.syncNow() } }
            Button(sync.paired ? "Disconnect" : "Sign out", role: .destructive) { confirmDisconnect = true }
        } header: {
            Text("Sync")
        } footer: {
            Text(sync.paired
                 ? "This phone keeps its own copy of your decks and syncs with Recall on your computer whenever both are online. Changes made offline sync later."
                 : "This phone keeps its own copy of your decks and syncs with your account.")
        }
    }
}

/// Pairing with a computer: scan the code in Recall's Settings there (or from `recall host pair`), or paste it.
struct PairView: View {
    @Environment(Library.self) private var library
    @Environment(\.dismiss) private var dismiss
    /// A ticket to pair with straight away, from a `recall://pair/…` link.
    var ticket: String?
    @State private var code = ""
    @State private var busy = false
    @State private var error: String?
    @State private var scanning = false

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    VStack(spacing: 12) {
                        Image(systemName: "laptopcomputer.and.iphone")
                            .font(.system(size: 30))
                            .foregroundStyle(.white)
                            .frame(width: 72, height: 72)
                            .background(Color.accentColor.gradient, in: .rect(cornerRadius: 18))
                        Text("On your computer, open Recall’s Settings and choose **Pair a phone**. Then scan the code it shows.")
                            .foregroundStyle(.secondary)
                        if QRScanner.available {
                            Button { scanning = true } label: {
                                Label("Scan code", systemImage: "qrcode.viewfinder").frame(maxWidth: .infinity)
                            }
                            .buttonStyle(.glassProminent)
                            .controlSize(.large)
                            .padding(.top, 8)
                            .disabled(busy)
                        }
                    }
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: .infinity)
                    .listRowBackground(Color.clear)
                }

                Section {
                    TextField("rchub1…", text: $code, axis: .vertical)
                        .lineLimit(1...4)
                        .font(.footnote.monospaced())
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    PasteButton(payloadType: String.self) { strings in
                        guard let pasted = strings.first else { return }
                        Task { @MainActor in
                            code = pasted
                            if pairingTicket(pasted) != nil { await pair(pasted) }
                        }
                    }
                    .labelStyle(.titleAndIcon)
                    Button {
                        Task { await pair(code) }
                    } label: {
                        HStack {
                            Text("Pair")
                            if busy {
                                Spacer()
                                ProgressView()
                            }
                        }
                    }
                    .disabled(busy || pairingTicket(code) == nil)
                } header: {
                    Text("Or paste the code")
                } footer: {
                    if let error {
                        Text(error).foregroundStyle(.red)
                    } else {
                        Text("From a terminal, `recall host pair` prints one too. Each code works once, for 10 minutes.")
                    }
                }
            }
            .scrollDismissesKeyboard(.interactively)
            .navigationTitle("Pair with your computer")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button(role: .cancel) { dismiss() } }
            }
            .sheet(isPresented: $scanning) {
                NavigationStack {
                    QRScanner { scanned in
                        scanning = false
                        code = scanned
                        Task { await pair(scanned) }
                    }
                    .ignoresSafeArea()
                    .navigationTitle("Scan the code on your computer")
                    .navigationBarTitleDisplayMode(.inline)
                    .toolbar {
                        ToolbarItem(placement: .cancellationAction) { Button(role: .cancel) { scanning = false } }
                    }
                }
            }
            .task {
                if let ticket {
                    code = ticket
                    await pair(ticket)
                }
            }
        }
    }

    private func pair(_ text: String) async {
        guard let ticket = pairingTicket(text) else {
            error = "That isn’t a pairing code. Scan the one in Recall’s Settings on your computer."
            return
        }
        busy = true
        error = nil
        defer { busy = false }
        do {
            try await library.pair(ticket: ticket)
            dismiss()
        } catch let e as CoreError where e.code == "network" {
            error = "Couldn’t reach your computer. Check Recall is open there and online, then try again."
        } catch {
            self.error = error.localizedDescription
        }
    }
}

/// Signing in to Recall Cloud with an account token, or making a new account.
private struct CloudSignIn: View {
    @Environment(Library.self) private var library
    @Environment(\.dismiss) private var dismiss
    @State private var server = ""
    @State private var token = ""
    @State private var busy = false
    @State private var error: String?
    /// A new account's token, shown once so it can be kept for other devices.
    @State private var created: String?

    var body: some View {
        Form {
            if let created {
                Section {
                    Text(created).font(.footnote.monospaced()).textSelection(.enabled)
                    Button("Copy token") { UIPasteboard.general.string = created }
                    Button("Done") { dismiss() }
                } header: {
                    Text("Your account token")
                } footer: {
                    Text("It signs you in on your other devices. Keep it secret.")
                }
            } else {
                Section {
                    field("Server (recall.example.com)", text: $server).keyboardType(.URL)
                    field("rc_…", text: $token).monospaced()
                    Button("Sign in") { Task { await signIn(token: token) } }
                        .disabled(busy || url == nil || token.trimmingCharacters(in: .whitespaces).isEmpty)
                    Button("Create a new account") { Task { await signIn(token: nil) } }
                        .disabled(busy || url == nil)
                } footer: {
                    if let error { Text(error).foregroundStyle(.red) }
                }
            }
        }
        .navigationTitle("Recall Cloud")
        .navigationBarTitleDisplayMode(.inline)
    }

    /// The server as typed, HTTPS unless it says otherwise.
    private var url: String? {
        var server = server.trimmingCharacters(in: .whitespacesAndNewlines)
        while server.hasSuffix("/") { server.removeLast() }
        guard !server.isEmpty else { return nil }
        if !server.contains("://") { server = "https://" + server }
        return URLComponents(string: server)?.host?.isEmpty == false ? server : nil
    }

    private func field(_ prompt: String, text: Binding<String>) -> some View {
        TextField(prompt, text: text)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
    }

    private func signIn(token: String?) async {
        guard let url else { return }
        busy = true
        error = nil
        defer { busy = false }
        do {
            let token = token?.trimmingCharacters(in: .whitespacesAndNewlines)
            let kept = try await library.signIn(server: url, token: token)
            if token == nil { created = kept } else { dismiss() }
        } catch let e as CoreError where e.code == "unauthorized" {
            error = "That token wasn’t accepted"
        } catch {
            self.error = error.localizedDescription
        }
    }
}

/// The camera, looking for the pairing code in the computer's Settings.
struct QRScanner: UIViewControllerRepresentable {
    let found: (String) -> Void

    @MainActor static var available: Bool {
        DataScannerViewController.isSupported && DataScannerViewController.isAvailable
    }

    func makeUIViewController(context: Context) -> DataScannerViewController {
        let scanner = DataScannerViewController(
            recognizedDataTypes: [.barcode(symbologies: [.qr])], qualityLevel: .balanced, isHighlightingEnabled: true)
        scanner.delegate = context.coordinator
        return scanner
    }

    func updateUIViewController(_ scanner: DataScannerViewController, context: Context) {
        if !scanner.isScanning { try? scanner.startScanning() }
    }

    func makeCoordinator() -> Coordinator { Coordinator(found: found) }

    @MainActor
    final class Coordinator: NSObject, DataScannerViewControllerDelegate {
        let found: (String) -> Void
        private var done = false

        init(found: @escaping (String) -> Void) { self.found = found }

        func dataScanner(
            _ scanner: DataScannerViewController, didAdd added: [RecognizedItem], allItems: [RecognizedItem]
        ) {
            for item in added {
                guard !done, case .barcode(let code) = item, let value = code.payloadStringValue,
                      pairingTicket(value) != nil
                else { continue }
                done = true
                scanner.stopScanning()
                found(value)
                return
            }
        }
    }
}

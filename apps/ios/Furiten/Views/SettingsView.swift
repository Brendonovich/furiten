import SwiftUI
import VisionKit

/// Settings: where this phone's library syncs. The library lives on the phone either way; pairing with your computer
/// keeps it in step with your other devices.
struct SettingsView: View {
    @Environment(Library.self) private var library
    @Environment(\.dismiss) private var dismiss
    @Environment(\.palette) private var palette
    @AppStorage(Theme.key) private var theme = Theme.system
    @State private var pairing = false
    @State private var confirmDisconnect = false

    /// The hour the review day rolls over at: what the library says, 4am until it loads, sent back when it changes.
    private var rollover: Binding<Int> {
        Binding(
            get: { library.settings?.rolloverHour ?? 4 },
            set: { hour in Task { await library.setRollover(hour) } }
        )
    }

    var body: some View {
        NavigationStack {
            Form {
                Group {
                    if let sync = library.sync, sync.signedIn {
                        syncing(sync)
                    } else {
                        notSyncing
                    }
                    Section {
                        Picker("Theme", selection: $theme) {
                            ForEach(Theme.allCases) { Text($0.name).tag($0) }
                        }
                    } header: {
                        Text("Appearance")
                    } footer: {
                        Text(theme == .furiten
                             ? "Furiten’s own colours, as on the web, always dark."
                             : "The system’s colours, light or dark as your phone is.")
                    }
                    Section {
                        Picker("Next day starts at", selection: rollover) {
                            ForEach(0..<24, id: \.self) { hour in
                                Text(String(format: "%02d:00", hour)).tag(hour)
                            }
                        }
                    } header: {
                        Text("Review")
                    } footer: {
                        Text("The day’s due cards are all there from the hour you pick, and reviews before it count as the day before, so late-night studying stays yesterday’s.")
                    }
                    Section {
                        LabeledContent("Version", value: Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "")
                    }
                }
                .listRowBackground(palette.row)
            }
            .listBackground(palette.list)
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { Button(role: .close) { dismiss() } }
            }
            .sheet(isPresented: $pairing) { PairView() }
            .confirmationDialog(
                "Stop syncing with your computer?",
                isPresented: $confirmDisconnect, titleVisibility: .visible
            ) {
                Button("Disconnect", role: .destructive) {
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
                    Text("Your decks live on this phone, so reviewing and editing work anywhere, even offline. Pair it with Furiten on your computer to keep the two in step: they sync whenever both are online, on any network.")
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
        }
    }

    private func syncing(_ sync: CoreStatus) -> some View {
        Section {
            LabeledContent("Your computer") {
                HStack(spacing: 6) {
                    Circle().fill(sync.connected == true ? .green : .orange).frame(width: 7, height: 7)
                    Text(sync.connected == true ? "Connected" : "Not connected")
                }
            }
            LabeledContent("Last synced", value: sync.lastSync.map { ago(Date(ms: $0)) } ?? "Not synced yet")
            if let error = sync.syncError {
                Text(error).font(.footnote).foregroundStyle(.red)
            }
            Button("Sync now") { Task { await library.syncNow() } }
            Button("Disconnect", role: .destructive) { confirmDisconnect = true }
        } header: {
            Text("Sync")
        } footer: {
            Text("This phone keeps its own copy of your decks and syncs with Furiten on your computer whenever both are online. Changes made offline sync later.")
        }
    }
}

/// Pairing with a computer: scan the code in Furiten's Settings there (or from `furiten host pair`), or paste it.
struct PairView: View {
    @Environment(Library.self) private var library
    @Environment(\.dismiss) private var dismiss
    @Environment(\.palette) private var palette
    /// A ticket to pair with straight away, from a `furiten://pair/…` link.
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
                            .foregroundStyle(palette.onAccent)
                            .frame(width: 72, height: 72)
                            .background(palette.accent.gradient, in: .rect(cornerRadius: 18))
                        Text("On your computer, open Furiten’s Settings and choose **Pair a phone**. Then scan the code it shows.")
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
                        Text("From a terminal, `furiten host pair` prints one too. Each code works once, for 10 minutes.")
                    }
                }
                .listRowBackground(palette.row)
            }
            .listBackground(palette.list)
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
            error = "That isn’t a pairing code. Scan the one in Furiten’s Settings on your computer."
            return
        }
        busy = true
        error = nil
        defer { busy = false }
        do {
            try await library.pair(ticket: ticket)
            dismiss()
        } catch let e as CoreError where e.code == "network" {
            error = "Couldn’t reach your computer. Check Furiten is open there and online, then try again."
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

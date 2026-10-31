# Syncing over Iroh for private deployments

*2026-10-31.*

**Decided:** self-hosting is `recall-hub`, a Rust binary networked with Iroh that acts as an always-online hub, the
same role the Durable Object plays on Recall Cloud (TypeScript, Cloudflare). Devices sync with one hub, never with
each other. Iroh is the transport only. The design is in `docs/contract.md` ("Self-hosted hub"). The notes below
are how we got there; the mesh and peer-list parts are not being built.

Prior art: Deadbolt (a password manager) syncs device-to-device over Iroh, with signed membership and PAKE pairing
codes. That fits a password manager, where any one online device is enough. A review habit needs the phone and
the laptop to converge even when they're never online together, hence a hub. Deadbolt's connection limits,
on-accept-and-dial membership checks and pairing code design are worth borrowing for the hub.

## Question

Can devices sync directly with each other over [Iroh](https://www.iroh.computer) instead of through the Cloudflare
Durable Object, so someone can run Recall without our server ever holding their cards?

## Where Iroh is

- iroh 1.0 shipped 2026-06-15 with a stable wire protocol and API; current is 1.4.1 (2026-10-21). Swift and Kotlin
  bindings are official again, and it runs in the browser as wasm (relay-only, since browsers have no UDP).
- Connections are QUIC, dialled by public key, end-to-end encrypted, direct where hole punching works and through a
  relay otherwise. Relays only see ciphertext. `iroh-relay` is a single open-source binary, and 1.0 added bearer
  token access control, so a private deployment can run its own.
- LAN discovery is mDNS (`iroh-mdns-address-lookup`); internet discovery is DNS/pkarr.
- `iroh-docs` (CRDT key-value documents) and `iroh-gossip` are still 0.101 and outside the 1.0 stability promise.

## Two ways to build it

**A. Store cards in `iroh-docs`.** Map `deck/<id>`, `card/<id>`, `review/<id>` to entries; its latest-per-key LWW
and set reconciliation match our merge rules. But it brings redb, iroh-blobs and gossip alongside our SQLite, so we'd
keep two copies of everything or give up SQLite queries, and it isn't covered by 1.0's stability promise.

**B. Iroh as a transport for our own protocol (preferred).** Our sync is already a state-based CRDT: LWW per deck and
card, union for reviews, and schedules derived by replaying reviews. None of that cares who the other side is. Make it
symmetric:

- Every change gets a local `seq` when it enters a store, whether it was written locally or received.
- Each peer remembers, per remote peer, the highest remote `seq` it has applied.
- On connect, over ALPN `recall/sync/1`: each side sends `{ since }`, streams its changes after the other's `since`,
  and both apply them with the same merge function the server uses. `POST /v1/sync` is this protocol with one side
  fixed as the hub.
- While connected, push new changes as they're written, so two open devices update live.

One dependency (`iroh`), one merge implementation, and the cloud and P2P paths can't drift apart.

## Pairing and trust

- `recall pair` (or a QR code in the app) shows a ticket: endpoint ID, relay URL and a one-time secret.
- The other device dials it, proves the secret, and both add each other's endpoint ID to a **peer list that syncs
  like any other data**, so pairing a phone with your laptop also pairs it with your desktop.
- Incoming connections from endpoint IDs not on the list are refused before any data moves (Iroh's endpoint hooks).
- Removing a device removes it from the list everywhere. It keeps what it already has, as with any sync.

## The catch: two devices are rarely online together

Pure P2P only syncs when two devices are online at the same time. A phone reviewed on the train and a laptop that's
asleep never meet. The answer is an **always-on peer**: the same `recall core` binary run headless
(`recall core --headless`) on a home server, NAS, Raspberry Pi or a small VPS. It's just another peer, so there's no
second server codebase, and it's the natural home for a private deployment's HTTP API and MCP endpoint.

That raises the real cost: today the HTTP API and MCP live in the TypeScript Worker. A headless core serving them
means a second implementation in Rust, unless the Worker becomes a thin layer and the API moves into the core
everywhere. That's worth deciding before the API grows.

## Other costs

- **Size and runtime.** iroh pulls in tokio, rustls and its QUIC stack. The core becomes async (or runs a tokio
  runtime on a thread). Fine for the desktop core and mobile; irrelevant to the Rune app, which only talks to the
  socket.
- **Clocks.** With many peers, wall-clock LWW is more exposed to skewed clocks. Switch `updated_at` to a hybrid
  logical clock (still ms-compatible) before shipping P2P.
- **iOS.** Sync happens only while the app is open or in short background tasks; with an always-on peer that's enough.
- **Web.** A browser can be a peer through a relay, but it's the weakest platform for it; it stays on the HTTP API.

## Proposal

Offer both, picked per account in settings: **Recall Cloud** (the Worker, zero setup, the default) and **Private**
(paired devices over Iroh, optionally with a headless always-on peer and a self-hosted relay). Concretely:

1. Make the core's sync transport-agnostic now, while it's being written: the merge function, a per-peer cursor table,
   and the symmetric exchange, with HTTP as the first transport. Cheap today, a rewrite later.
2. Spike `recall pair`, `recall peers` and `recall/sync/1` behind an `iroh` feature: two cores on one machine converge
   over LAN and through the n0 relay, and a test proves a third peer gets changes through the always-on one.
3. Then decide where the HTTP API and MCP live for private deployments.

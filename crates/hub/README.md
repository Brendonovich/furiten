# recall-hub

`recall-hub` holds one Recall library on a machine you own (a NAS, a Raspberry Pi, a small VPS) and syncs it with
your paired devices. Devices only ever sync with the hub, never with each other, so a phone and a laptop that are
never online together still converge. It does the same job as Recall Cloud, with the same merge rules.

It connects over [Iroh](https://www.iroh.computer): each side is dialled by its public key, connections are
end-to-end encrypted, direct where possible and through a relay otherwise. You don't open ports, set up DNS or get
certificates. Only devices you paired can connect. The protocol is in `docs/contract.md` ("Self-hosted hub").

## Install

```sh
cargo install --locked --path crates/hub      # from a checkout; puts recall-hub in ~/.cargo/bin
```

Or build once with `cargo build --release -p recall-hub` and copy `target/release/recall-hub` to the server.

## Run

```sh
recall-hub serve
```

It prints its endpoint ID and library ID, then serves until you stop it with Ctrl-C or SIGTERM. Everything lives in
the data directory, `$XDG_DATA_HOME/recall-hub` (default `~/.local/share/recall-hub`; on macOS
`~/Library/Application Support/recall-hub`), or wherever `--data <dir>` points:

| File | |
| --- | --- |
| `iroh.key` | the hub's secret key (mode 0600). This is its identity: keep it, back it up, don't share it. |
| `library.db` | the library (SQLite). Back it up with the key. |
| `devices.json` | paired devices (mode 0600). |
| `hub.sock` | the control socket `pair`, `status` and `devices` use while the hub runs (mode 0600). |

Only one hub runs per data directory.

### As a systemd service

```ini
# /etc/systemd/system/recall-hub.service
[Unit]
Description=Recall hub
After=network-online.target
Wants=network-online.target

[Service]
User=recall
ExecStart=/usr/local/bin/recall-hub --data /var/lib/recall-hub serve
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

```sh
sudo useradd --system --home /var/lib/recall-hub --create-home recall
sudo systemctl enable --now recall-hub
sudo -u recall recall-hub --data /var/lib/recall-hub pair
```

Run `pair`, `status` and `devices` as the same user with the same `--data`, since the control socket is private to
that user. On macOS, a launchd agent running `recall-hub serve` with `KeepAlive` works the same way.

## Pair a device

On the hub:

```sh
recall-hub pair
```

This prints a ticket (`rchub1...`) and, in a terminal, a QR code of it. A ticket works once, for 10 minutes, and only
while the hub that made it keeps running. On the device:

```sh
recall sync login --hub rchub1... --name laptop
```

The device makes its own key (`$RECALL_HOME/iroh.key`), pairs, syncs everything it has, and from then on keeps a
connection to the hub while its core runs. It pushes about 2 seconds after each change, and pulls as soon as the hub
says another device changed something. `recall status` shows the hub and whether it's connected.

## Devices

```sh
recall-hub devices              # list, with which are connected
recall-hub devices rm laptop    # by name, id or id prefix; closes its connections
recall-hub status               # endpoint, library, network, devices
```

Add `--json` to any of these for scripts. A removed device keeps the cards it already has but can't sync until it's
paired again with a new ticket. `devices` and `status` also work while the hub is stopped.

## Relays

By default the hub and devices use n0's public relays, and the hub publishes its address through n0's DNS so
devices find it again if it moves. Relays only see encrypted traffic, and most connections end up direct anyway.

To avoid n0 entirely, run your own [`iroh-relay`](https://github.com/n0-computer/iroh/tree/main/iroh-relay) and
point the hub at it:

```sh
recall-hub serve --relay https://relay.example.com
```

Tickets the hub makes then carry that relay, and devices dial the hub through it. Set `RECALL_RELAY` to the same URL
where a device's core runs so it doesn't use n0's relays either. With a custom relay the hub doesn't publish to n0's
DNS, so the relay URL needs to stay the same (or pair devices again).

To try it out on one machine with no network at all, set `RECALL_NET=local` for the hub and the devices: loopback
only, no relays.

## Not yet

- **HTTP API and MCP** (`recall-hub --http 127.0.0.1:8787`, `recall-hub tokens create`). The store already supports
  it: a hub's store is a full Recall store, and local writes get sync sequence numbers, so it's the HTTP layer and
  `tests/conformance` that are missing. Milestone 5.
- A fixed UDP port (`--bind`) for LAN-only setups without a relay.

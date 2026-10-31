//! A hub and devices on loopback, with no relays or address lookup: offline and headless.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use iroh::SecretKey;
use recall_core::client::Client;
use recall_core::hub::{self, HubLink, Net, Ticket};
use recall_core::protocol::{Answer, CardQuery, CardUpdate, NewCard, NewDeck};
use recall_core::sync::{lock, sync_once};
use recall_core::{Store, server::Server};
use recall_hub::{Config, Hub, control};
use serde_json::json;

const T0: i64 = 1_800_000_000_000;
const DAY: i64 = 86_400_000;
const WAIT: Duration = Duration::from_secs(15);

fn temp(prefix: &str) -> PathBuf {
    // Short: Unix socket paths are limited to about 100 bytes.
    let id = recall_core::model::ulid();
    std::env::temp_dir().join(format!("{prefix}-{}", &id[16..]))
}

struct TestHub {
    runtime: tokio::runtime::Runtime,
    hub: Option<Hub>,
    data: PathBuf,
}

impl TestHub {
    fn start(ticket_ttl: Duration) -> TestHub {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let data = temp("rh");
        let config = Config { ticket_ttl, ..Config::new(&data, Net::Local) };
        let hub = runtime.block_on(Hub::start(config)).unwrap();
        TestHub { runtime, hub: Some(hub), data }
    }

    fn hub(&self) -> &Hub {
        self.hub.as_ref().unwrap()
    }

    fn ticket(&self) -> Ticket {
        self.runtime.block_on(self.hub().pair()).unwrap().ticket.parse().unwrap()
    }
}

impl Drop for TestHub {
    fn drop(&mut self) {
        if let Some(hub) = self.hub.take() {
            self.runtime.block_on(hub.shutdown());
        }
        let _ = std::fs::remove_dir_all(&self.data);
    }
}

/// A device: its key and store, paired with the hub as `name`.
struct Device {
    key: SecretKey,
    store: Mutex<Store>,
}

impl Device {
    fn paired(hub: &TestHub, name: &str) -> Device {
        let key = SecretKey::generate();
        let library = hub::pair(key.clone(), &Net::Local, &hub.ticket(), name).unwrap();
        assert_eq!(library, hub.hub().library_id());
        let mut store = Store::open_in_memory().unwrap();
        store.set_now(Some(T0));
        Device { key, store: Mutex::new(store) }
    }

    fn link(&self, hub: &TestHub, on_changed: impl Fn() + Send + Sync + 'static) -> HubLink {
        let addr = hub::hub_addr(&iroh::EndpointAddr::new(hub.hub().endpoint_id()));
        let addr = recall_core::store::HubAddr { direct_addrs: hub.ticket().addr.direct_addrs, ..addr };
        HubLink::start(self.key.clone(), &Net::Local, &addr, on_changed).unwrap()
    }

    fn set_now(&self, now: i64) {
        lock(&self.store).set_now(Some(now));
    }
}

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < WAIT, "timed out waiting until {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_ticket_pairs_one_device_once_and_only_while_fresh() {
    let hub = TestHub::start(Duration::from_millis(500));
    let ticket = hub.ticket();
    assert!(ticket.to_string().starts_with("rchub1"));
    let pair = |ticket: &Ticket, name: &str| hub::pair(SecretKey::generate(), &Net::Local, ticket, name);

    assert_eq!(pair(&ticket, "  ").unwrap_err().code, "invalid_params", "a bad name doesn't spend the ticket");
    assert_eq!(pair(&ticket, "laptop").unwrap(), hub.hub().library_id());
    assert_eq!(pair(&ticket, "phone").unwrap_err().code, "ticket_used");

    let mut wrong = hub.ticket();
    wrong.secret[0] ^= 1;
    assert_eq!(pair(&wrong, "phone").unwrap_err().code, "bad_ticket");

    let stale = hub.ticket();
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(pair(&stale, "phone").unwrap_err().code, "ticket_expired");

    let devices = hub.hub().devices();
    assert_eq!(devices.iter().map(|d| d.device.name.as_str()).collect::<Vec<_>>(), ["laptop"]);
    // The device list survives a restart.
    assert_eq!(recall_hub::data::Devices::load(&hub.data).unwrap().list().len(), 1);
}

#[test]
fn a_device_that_is_not_paired_is_refused() {
    let hub = TestHub::start(recall_hub::TICKET_TTL);
    let stranger = Device { key: SecretKey::generate(), store: Mutex::new(Store::open_in_memory().unwrap()) };
    let link = stranger.link(&hub, || {});
    let error = sync_once(&stranger.store, &link).unwrap_err();
    assert_eq!(error.code, "unauthorized", "{error}");
    assert!(!link.is_connected());
    assert_eq!(hub.hub().status().unwrap().connected, 0);
}

#[test]
fn two_devices_converge_through_the_hub_after_offline_edits_and_reviews() {
    let hub = TestHub::start(recall_hub::TICKET_TTL);
    let (a, b) = (Device::paired(&hub, "a"), Device::paired(&hub, "b"));
    let (link_a, link_b) = (a.link(&hub, || {}), b.link(&hub, || {}));
    let sync = |d: &Device, link: &HubLink| sync_once(&d.store, link).unwrap().0;

    let deck = lock(&a.store).create_deck(NewDeck { name: "Spanish".into(), parent_id: None }).unwrap();
    let card = lock(&a.store)
        .create_card(NewCard { deck_id: deck.id.clone(), content: "hola\n---\nhello".into(), tags: vec![] })
        .unwrap();
    let id = card.card.id;
    assert_eq!(sync(&a, &link_a).pushed, 8);
    assert_eq!(sync(&b, &link_b).pulled, 2, "the sample deck both have isn't sent back");

    // Offline on both: conflicting edits (b's is later) and reviews interleaved in time.
    let edit = |d: &Device, at: i64, content: &str| {
        d.set_now(at);
        lock(&d.store)
            .update_card(CardUpdate { id: id.clone(), content: Some(content.into()), ..Default::default() })
            .unwrap();
    };
    let review = |d: &Device, at: i64, rating: u8| {
        d.set_now(at);
        lock(&d.store).answer(Answer { card_id: id.clone(), rating, duration_ms: None }).unwrap();
    };
    edit(&a, T0 + 10, "from a");
    edit(&b, T0 + 20, "from b");
    review(&a, T0 + DAY, 4);
    review(&b, T0 + 2 * DAY, 1);
    review(&a, T0 + 3 * DAY, 3);

    sync(&a, &link_a);
    sync(&b, &link_b);
    sync(&a, &link_a);
    let everything = |d: &Device| lock(&d.store).cards(&CardQuery { limit: Some(1000), ..Default::default() }).unwrap();
    assert_eq!(everything(&a), everything(&b));
    let card = lock(&a.store).card(&id).unwrap();
    assert_eq!(card.card.content, "from b");
    assert_eq!((card.schedule.reps, card.schedule.lapses), (3, 1));
    assert_eq!(sync(&a, &link_a), sync(&b, &link_b), "nothing left to move");
    assert_eq!(sync(&a, &link_a).pulled, 0);
}

#[test]
fn a_connected_device_hears_about_other_devices_writes() {
    let hub = TestHub::start(recall_hub::TICKET_TTL);
    let (a, b) = (Device::paired(&hub, "a"), Device::paired(&hub, "b"));
    let (tx_a, rx_a) = mpsc::channel();
    let (tx_b, rx_b) = mpsc::channel();
    let link_a = a.link(&hub, move || tx_a.send(()).unwrap());
    let link_b = b.link(&hub, move || tx_b.send(()).unwrap());
    // Each link reports its first connection so the device catches up.
    rx_a.recv_timeout(WAIT).unwrap();
    rx_b.recv_timeout(WAIT).unwrap();
    wait_until("both are connected", || hub.hub().status().unwrap().connected == 2);

    lock(&b.store).create_deck(NewDeck { name: "From b".into(), parent_id: None }).unwrap();
    sync_once(&b.store, &link_b).unwrap();
    rx_a.recv_timeout(WAIT).expect("a hears about b's write");
    assert!(rx_b.recv_timeout(Duration::from_millis(300)).is_err(), "b isn't told about its own write");
    sync_once(&a.store, &link_a).unwrap();
    assert!(lock(&a.store).decks().unwrap().iter().any(|d| d.deck.name == "From b"));

    // Syncing nothing new tells nobody.
    sync_once(&b.store, &link_b).unwrap();
    assert!(rx_a.recv_timeout(Duration::from_millis(300)).is_err());
}

#[test]
fn removing_a_device_closes_its_connection() {
    let hub = TestHub::start(recall_hub::TICKET_TTL);
    let a = Device::paired(&hub, "phone");
    let link = a.link(&hub, || {});
    wait_until("the phone is connected", || link.is_connected());
    sync_once(&a.store, &link).unwrap();
    assert!(hub.hub().devices()[0].connected);

    // Through the control socket, as `recall-hub devices rm phone` does.
    let status = control::call(&hub.data, "status", json!({})).unwrap().unwrap();
    assert_eq!(status["connected"], 1);
    let removed = control::call(&hub.data, "devices.rm", json!({ "device": "PHONE" })).unwrap().unwrap();
    assert_eq!(removed["name"], "phone");
    wait_until("the phone is cut off", || !link.is_connected());
    assert_eq!(sync_once(&a.store, &link).unwrap_err().code, "unauthorized");
    assert!(hub.hub().devices().is_empty());
    let error = control::call(&hub.data, "devices.rm", json!({ "device": "phone" })).unwrap_err();
    assert_eq!(error.code, "not_found");
}

#[test]
fn two_cores_pair_with_a_hub_and_see_each_others_cards_live() {
    let hub = TestHub::start(recall_hub::TICKET_TTL);
    let homes = [temp("rc"), temp("rc")];
    let cores: Vec<_> = homes
        .iter()
        .map(|home| {
            let server = Server::bind_with_net(home, Net::Local).unwrap();
            std::thread::spawn(move || server.serve())
        })
        .collect();
    let mut a = Client::connect(&homes[0]).unwrap();
    let mut b = Client::connect(&homes[1]).unwrap();
    assert_eq!(a.login_hub(&hub.ticket().to_string(), Some("desk")).unwrap().library_id, hub.hub().library_id());
    b.login_hub(&hub.ticket().to_string(), None).unwrap();
    assert_eq!(a.login_hub("rchub1nope", None).unwrap_err().code, "bad_ticket");

    let status = a.status().unwrap();
    assert_eq!(status.server.unwrap(), format!("hub:{}", hub.hub().endpoint_id()));
    wait_until("a is connected", || a.status().unwrap().connected == Some(true));
    wait_until("b is connected", || b.status().unwrap().connected == Some(true));
    let key = std::fs::metadata(homes[0].join("iroh.key")).unwrap();
    assert_eq!(std::os::unix::fs::PermissionsExt::mode(&key.permissions()) & 0o777, 0o600);

    let deck = a.resolve_deck("Live", true).unwrap();
    let card = a.create_card(&NewCard { deck_id: deck.id, content: "q\n---\na".into(), tags: vec![] }).unwrap();
    // Well within the 60 s periodic sync: a pushes after its 2 s debounce, the hub tells b, b pulls.
    wait_until("b has a's card", || b.card(&card.card.id).is_ok());
    b.answer(&card.card.id, 3, None).unwrap();
    wait_until("a has b's review", || a.card(&card.card.id).unwrap().schedule.reps == 1);

    let names: Vec<String> = hub.hub().devices().into_iter().map(|d| d.device.name).collect();
    assert_eq!(names[0], "desk");
    assert_eq!(names[1], hub::default_device_name());

    for client in [&mut a, &mut b] {
        client.shutdown().unwrap();
    }
    for core in cores {
        core.join().unwrap().unwrap();
    }
    for home in homes {
        std::fs::remove_dir_all(home).unwrap();
    }
}

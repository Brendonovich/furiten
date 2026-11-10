use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use super::*;
use crate::client::{Client, Event, Events};

fn home() -> PathBuf {
    // Short: Unix socket paths are limited to about 100 bytes.
    let id = crate::model::ulid();
    std::env::temp_dir().join(format!("rc-{}", &id[16..]))
}

fn start(home: &Path) -> std::thread::JoinHandle<Result<()>> {
    let server = Server::bind(home).unwrap();
    std::thread::spawn(move || server.serve())
}

#[test]
fn clients_talk_to_the_core_and_hear_about_changes() {
    let home = home();
    let core = start(&home);
    let mode = std::fs::metadata(socket_path(&home)).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);

    let mut a = Client::connect(&home).unwrap();
    let mut b = Client::connect(&home).unwrap();
    let (tx, rx) = mpsc::channel();
    let events = a.events().unwrap();
    std::thread::spawn(move || {
        for event in events {
            tx.send(event).unwrap();
        }
    });

    let status = a.status().unwrap();
    assert_eq!((status.version.as_str(), status.signed_in, status.server), (VERSION, false, None));
    let deck = a.create_deck("Spanish", None).unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), Event::Changed);

    let cards = b
        .create_cards(&[
            NewCard { deck_id: deck.id.clone(), content: "hola\n---\nhello".into(), tags: vec!["Greeting".into()] },
            NewCard { deck_id: deck.id.clone(), content: "adiós\n---\ngoodbye".into(), tags: vec![] },
        ])
        .unwrap();
    assert_eq!(cards[0].card.tags, ["greeting"]);
    // `a` still gets replies even though events for b's write are queued on its connection.
    assert_eq!(a.decks().unwrap().len(), 2);
    let next = a.next_review(Some(&deck.id)).unwrap();
    assert_eq!(next.remaining, Remaining { due: 0, new: 2 });
    let answered = a.answer(&next.card.unwrap().card.id, true, Some(1500)).unwrap();
    assert_eq!(answered.schedule.state, crate::State::Learning);
    assert_eq!(b.stats(Some(&deck.id)).unwrap().reviews_today, 1);
    assert_eq!(b.resolve_deck("spanish", false).unwrap().id, deck.id);
    assert_eq!(b.resolve_deck("French", true).unwrap().name, "French");

    let err = a.card("nope").unwrap_err();
    assert_eq!(err.code, "not_found");
    let err = a.call::<Value>("nope", json!({})).unwrap_err();
    assert_eq!(err.code, "unknown_method");
    let err = a.call::<Value>("cards.create", json!({ "content": 1 })).unwrap_err();
    assert_eq!(err.code, "invalid_params");
    assert_eq!(a.sync_now().unwrap_err().code, "not_signed_in");

    // The second instance refuses while the first runs.
    assert_eq!(Server::bind(&home).err().unwrap().code, "already_running");

    a.shutdown().unwrap();
    core.join().unwrap().unwrap();
    assert!(!socket_path(&home).exists());
    assert!(Client::connect(&home).is_err());

    // A restart (here over a stale socket file) keeps the data.
    std::fs::write(socket_path(&home), b"").unwrap();
    let core = start(&home);
    let mut c = Client::connect(&home).unwrap();
    assert_eq!(c.cards(&CardQuery { deck_id: Some(deck.id), ..Default::default() }).unwrap().len(), 2);
    c.shutdown().unwrap();
    core.join().unwrap().unwrap();
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn the_hosting_app_opens_its_window_and_stops_the_core() {
    let home = home();
    let core = start(&home);
    let mut c = Client::connect(&home).unwrap();
    assert_eq!(c.open_app().unwrap_err().code, "unsupported", "no app hosts this core");
    c.shutdown().unwrap();
    core.join().unwrap().unwrap();

    let server = Server::bind(&home).unwrap();
    let (opened, opens) = mpsc::channel();
    server.on_open(move || opened.send(()).unwrap());
    let (exited, exits) = mpsc::channel();
    let hosted = server.spawn(move || exited.send(()).unwrap());
    let mut c = Client::connect(&home).unwrap();
    c.open_app().unwrap();
    opens.recv_timeout(Duration::from_secs(5)).unwrap();
    hosted.stop().unwrap();
    exits.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(!socket_path(&home).exists());
    assert!(Client::connect(&home).is_err());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn an_embedded_core_answers_lines_and_reports_changes() {
    let home = home();
    let (changed, changes) = mpsc::channel();
    let core = Embedded::open(&home, move || changed.send(()).unwrap()).unwrap();
    assert_eq!(Server::bind(&home).err().unwrap().code, "already_running");

    let reply: Value = serde_json::from_str(&core.request(r#"{"id":1,"method":"decks.list"}"#)).unwrap();
    assert_eq!(reply["id"], 1);
    assert_eq!(reply["result"][0]["name"], "Furiten basics", "a new store starts with the sample deck");
    let reply: Value =
        serde_json::from_str(&core.request(r#"{"id":2,"method":"decks.create","params":{"name":"Spanish"}}"#)).unwrap();
    assert_eq!(reply["result"]["name"], "Spanish");
    changes.recv_timeout(Duration::from_secs(5)).unwrap();

    let reply: Value = serde_json::from_str(&core.request("not json")).unwrap();
    assert_eq!(reply["error"]["code"], "bad_request");
    let reply: Value = serde_json::from_str(&core.request(r#"{"id":3,"method":"shutdown"}"#)).unwrap();
    assert_eq!(reply["error"]["code"], "unsupported");

    drop(core);
    let core = Embedded::open(&home, || {}).unwrap();
    let reply: Value = serde_json::from_str(&core.request(r#"{"id":4,"method":"decks.list"}"#)).unwrap();
    assert_eq!(reply["result"].as_array().unwrap().len(), 2, "the data outlives the core");
    drop(core);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_client_that_never_reads_does_not_block_the_core() {
    let home = home();
    let core = start(&home);
    let _idle = Events::connect(&home).unwrap();
    let mut c = Client::connect(&home).unwrap();
    let deck = c.create_deck("Busy", None).unwrap();
    for i in 0..2000 {
        c.update_deck(&DeckUpdate { id: deck.id.clone(), name: Some(format!("Busy {i}")), parent_id: None }).unwrap();
    }
    c.shutdown().unwrap();
    core.join().unwrap().unwrap();
    std::fs::remove_dir_all(home).unwrap();
}

#[cfg(feature = "import")]
#[test]
fn a_mochi_export_imports_through_the_core() {
    let home = home();
    let core = start(&home);
    let mut c = Client::connect(&home).unwrap();
    let file = home.join("data.edn");
    std::fs::write(&file, r#"{:version 2 :decks [{:name "Mochi" :cards [{:content "hola\n---\nhello"}]}]}"#).unwrap();
    let imported = c.import_mochi(&file, None).unwrap();
    assert_eq!((imported.decks.len(), imported.cards), (1, 1));
    assert_eq!(c.resolve_deck("Mochi", false).unwrap().id, imported.decks[0].id);
    let err = c.call::<Value>("import.mochi", json!({ "path": "data.edn" })).unwrap_err();
    assert_eq!(err.code, "invalid_params", "a relative path means nothing to the core");
    assert_eq!(c.import_mochi(&home.join("missing.mochi"), None).unwrap_err().code, "not_found");
    c.shutdown().unwrap();
    core.join().unwrap().unwrap();
    std::fs::remove_dir_all(home).unwrap();
}

#[cfg(feature = "web")]
mod web_access {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};

    use super::*;

    fn free_port() -> u16 {
        TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
    }

    /// One HTTP/1.1 request over a fresh connection: the status and the JSON body.
    fn http(port: u16, method: &str, path: &str, token: Option<&str>, body: Option<Value>) -> (u16, Value) {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let body = body.map(|b| b.to_string()).unwrap_or_default();
        let auth = token.map(|t| format!("Authorization: Bearer {t}\r\n")).unwrap_or_default();
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut raw = String::new();
        stream.read_to_string(&mut raw).unwrap();
        let (head, body) = raw.split_once("\r\n\r\n").unwrap();
        let status = head.split(' ').nth(1).unwrap().parse().unwrap();
        (status, serde_json::from_str(body).unwrap_or(Value::Null))
    }

    fn call(c: &mut Client, method: &str, params: Value) -> WebStatus {
        c.call(method, params).unwrap()
    }

    #[test]
    fn the_desktop_serves_the_http_api_with_its_token() {
        let home = home();
        let port = free_port();
        let server = Server::bind(&home).unwrap();
        server.serve_web(None);
        let hosted = server.spawn(|| {});
        let mut c = Client::connect(&home).unwrap();

        let off = call(&mut c, "web.status", json!({}));
        assert!(!off.enabled && !off.running && off.token.is_none());
        let on = call(&mut c, "web.enable", json!({ "bind": "127.0.0.1", "port": port }));
        assert!(on.enabled && on.running);
        assert_eq!(on.urls, [format!("http://localhost:{port}")]);
        let token = on.token.clone().unwrap();
        assert_eq!(on.link.as_deref(), Some(format!("http://localhost:{port}/#token={token}").as_str()));
        let mode = std::fs::metadata(home.join("web.json")).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);

        assert_eq!(http(port, "GET", "/health", None, None).0, 200);
        let (status, body) = http(port, "GET", "/v1/decks", None, None);
        assert_eq!((status, body["error"]["code"].as_str()), (401, Some("unauthorized")));
        assert_eq!(http(port, "GET", "/v1/decks", Some("rw_wrong"), None).0, 401);

        let t = Some(token.as_str());
        let (status, decks) = http(port, "GET", "/v1/decks", t, None);
        assert_eq!(status, 200);
        assert_eq!(decks["decks"][0]["name"], "Furiten basics");
        let (_, deck) = http(port, "POST", "/v1/decks", t, Some(json!({ "name": "Spanish" })));
        let deck = deck["id"].as_str().unwrap().to_string();

        // Events stream the changes made by any client.
        let mut events = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(events, "GET /v1/events?token={token} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut events = BufReader::new(events);
        let mut line = String::new();
        while line != ": connected\n" {
            line.clear();
            events.read_line(&mut line).unwrap();
        }

        let (status, card) = http(
            port,
            "POST",
            "/v1/cards",
            t,
            Some(json!({ "deck_id": deck, "content": "hola\n---\nhello", "tags": ["Greeting"] })),
        );
        assert_eq!((status, card["sides"][1].as_str()), (200, Some("hello")));
        loop {
            line.clear();
            events.read_line(&mut line).unwrap();
            if line == "data: changed\n" {
                break;
            }
        }
        let id = card["id"].as_str().unwrap();
        let (_, cards) = http(port, "GET", &format!("/v1/cards?deck_id={deck}&q=HOLA&sort=name&limit=5"), t, None);
        assert_eq!(cards["cards"].as_array().unwrap().len(), 1);
        let (_, next) = http(port, "GET", &format!("/v1/review/next?deck_id={deck}"), t, None);
        assert_eq!(next["remaining"], json!({ "due": 0, "new": 1 }));
        let (status, answered) = http(
            port,
            "POST",
            &format!("/v1/cards/{id}/reviews"),
            t,
            Some(json!({ "remembered": true, "duration_ms": 900 })),
        );
        assert_eq!((status, answered["schedule"]["reps"].as_u64()), (200, Some(1)));
        let (_, tags) = http(port, "GET", &format!("/v1/tags?deck_id={deck}"), t, None);
        assert_eq!(tags["tags"][0]["tag"], "greeting");
        let (_, stats) = http(port, "GET", "/v1/stats", t, None);
        assert_eq!(stats["reviews_today"], 1);
        let (_, patched) = http(port, "PATCH", &format!("/v1/cards/{id}"), t, Some(json!({ "content": "adiós" })));
        assert_eq!(patched["content"], "adiós");
        assert_eq!(http(port, "DELETE", &format!("/v1/cards/{id}"), t, None).1, json!({ "ok": true }));
        assert_eq!(http(port, "GET", &format!("/v1/cards/{id}"), t, None).0, 404);
        assert_eq!(http(port, "GET", "/v1/cards?limit=lots", t, None).0, 400);
        assert_eq!(http(port, "POST", "/v1/sync", t, Some(json!({}))).0, 404, "the desktop isn't a hub");

        // Turning it off stops listening; on again keeps the token.
        assert!(!call(&mut c, "web.disable", json!({})).running);
        assert!(TcpStream::connect(("127.0.0.1", port)).is_err());
        assert_eq!(call(&mut c, "web.enable", json!({})).token.unwrap(), token);
        // Restarting on every interface frees the port in time to bind it again.
        for _ in 0..2 {
            let again = call(&mut c, "web.enable", json!({ "bind": "0.0.0.0" }));
            assert!(again.running, "{:?}", again.error);
            assert_eq!(http(port, "GET", "/health", None, None).0, 200);
        }
        hosted.stop().unwrap();

        // A core the CLI hosts never listens, even when web access is on.
        let core = start(&home);
        let mut c = Client::connect(&home).unwrap();
        let status = call(&mut c, "web.status", json!({}));
        assert!(status.enabled && !status.running && status.error.is_some());
        assert!(TcpStream::connect(("127.0.0.1", port)).is_err());
        c.shutdown().unwrap();
        core.join().unwrap().unwrap();
        std::fs::remove_dir_all(home).unwrap();
    }
}

#[cfg(feature = "web")]
#[test]
fn web_tokens_are_random() {
    let (a, b) = (web::new_token().unwrap(), web::new_token().unwrap());
    assert!(a.starts_with("rw_") && a.len() == 35);
    assert_ne!(a[3..19], b[3..19]);
    assert_ne!(a[19..], b[19..]);
}

#[cfg(feature = "hub")]
mod hosting {
    use std::time::Instant;

    use super::*;
    use crate::hub::Net;

    fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
        let started = Instant::now();
        while !done() {
            assert!(started.elapsed() < Duration::from_secs(15), "timed out waiting until {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn desktop(home: &Path) -> Hosted {
        let server = Server::bind_with_net(home, Net::Local).unwrap();
        server.host_devices();
        server.spawn(|| {})
    }

    fn call(phone: &Embedded, method: &str, params: Value) -> Result<Value> {
        let line = json!({ "id": 1, "method": method, "params": params }).to_string();
        let mut reply: Value = serde_json::from_str(&phone.request(&line)).unwrap();
        match reply.get_mut("error") {
            Some(error) => Err(serde_json::from_value(error.take()).unwrap()),
            None => Ok(reply["result"].take()),
        }
    }

    #[test]
    fn a_phone_pairs_with_the_desktop_and_syncs_its_own_library() {
        let (desk_home, phone_home) = (home(), home());
        let mut desk_core = desktop(&desk_home);
        let mut desk = Client::connect(&desk_home).unwrap();
        let status = desk.host_status().unwrap();
        assert_eq!((status.enabled, status.running), (false, false));

        // Pairing turns hosting on.
        let ticket = desk.host_pair().unwrap();
        let status = desk.host_status().unwrap();
        assert!(status.enabled && status.running && status.error.is_none());
        assert_eq!(desk.login("https://example.com", Some("rc_u_s")).unwrap_err().code, "conflict");

        let phone = Embedded::open_with_net(&phone_home, Net::Local, || {}).unwrap();
        let paired = call(&phone, "sync.login", json!({ "hub": ticket.ticket, "name": "iPhone" })).unwrap();
        assert!(paired["library_id"].is_string());
        assert_eq!(
            call(&phone, "sync.login", json!({ "hub": ticket.ticket })).unwrap_err().code,
            "ticket_used",
            "a ticket pairs once"
        );
        wait_until("the phone is connected", || call(&phone, "status", json!({})).unwrap()["connected"] == true);
        assert_eq!(desk.host_status().unwrap().devices[0].device.name, "iPhone");

        // The phone writes to its own store, and the desktop hears about it within the debounce.
        let deck = call(&phone, "decks.create", json!({ "name": "Spanish" })).unwrap();
        let card =
            call(&phone, "cards.create", json!({ "deck_id": deck["id"], "content": "hola\n---\nhello" })).unwrap();
        let card_id = card["id"].as_str().unwrap().to_string();
        call(&phone, "review.answer", json!({ "card_id": card_id, "remembered": true })).unwrap();
        wait_until("the desktop has the phone's review", || desk.card(&card_id).is_ok_and(|c| c.schedule.reps == 1));

        // And the desktop's edits reach the phone, pushed by the host's `changed` event.
        desk.update_card(&CardUpdate {
            id: card_id.clone(),
            content: Some("hola\n---\nhi".into()),
            ..Default::default()
        })
        .unwrap();
        wait_until("the phone has the desktop's edit", || {
            call(&phone, "cards.get", json!({ "id": card_id })).unwrap()["content"] == "hola\n---\nhi"
        });

        // Signing out keeps the phone's library.
        call(&phone, "sync.logout", json!({})).unwrap();
        assert_eq!(call(&phone, "status", json!({})).unwrap()["signed_in"], false);
        assert!(call(&phone, "cards.get", json!({ "id": card_id })).is_ok());
        drop(phone);

        // Hosting and the device list outlive a restart of the app.
        desk_core.stop().unwrap();
        desk_core = desktop(&desk_home);
        let mut desk = Client::connect(&desk_home).unwrap();
        let status = desk.host_status().unwrap();
        assert!(status.running);
        assert_eq!(status.devices.len(), 1);
        assert_eq!(desk.host_remove("iphone").unwrap().name, "iPhone");
        assert!(desk.host_status().unwrap().devices.is_empty());

        let status = desk.host_disable().unwrap();
        assert_eq!((status.enabled, status.running), (false, false));
        desk_core.stop().unwrap();
        for home in [desk_home, phone_home] {
            std::fs::remove_dir_all(home).unwrap();
        }
    }
}

use super::*;

#[test]
fn tickets_roundtrip_and_reject_garbage() {
    let ticket = Ticket {
        addr: HubAddr {
            id: hex(&[7; 32]),
            relay_url: Some("https://relay.example.com./".into()),
            direct_addrs: vec!["127.0.0.1:4433".parse().unwrap(), "[::1]:9".parse().unwrap()],
        },
        secret: [9; 16],
    };
    let text = ticket.to_string();
    assert!(text.starts_with("rchub1"));
    assert!(text.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()));
    assert_eq!(text.parse::<Ticket>().unwrap(), ticket);
    assert_eq!(text.to_uppercase().parse::<Ticket>().unwrap(), ticket, "QR codes scan as uppercase");
    assert!(!format!("{ticket:?}").contains(&ticket.secret_hex()));

    let bare = Ticket { addr: HubAddr { id: hex(&[1; 32]), ..Default::default() }, secret: [0; 16] };
    assert_eq!(bare.to_string().parse::<Ticket>().unwrap(), bare);

    for bad in ["", "rchub1", "nope", &text[..text.len() - 3], &format!("{text}a"), "rchub1!!!!"] {
        assert_eq!(bad.parse::<Ticket>().unwrap_err().code, "bad_ticket", "{bad}");
    }
    for len in 0..12 {
        let bytes: Vec<u8> = (0..len).map(|i| (i * 37) as u8).collect();
        assert_eq!(ticket::roundtrip_base32(&bytes).unwrap(), bytes);
    }
}

#[test]
fn keys_are_created_once_with_mode_0600() {
    let dir = std::env::temp_dir().join(format!("furiten-key-{}", crate::model::ulid()));
    let path = dir.join("iroh.key");
    let key = load_or_create_key(&path).unwrap();
    assert_eq!(load_or_create_key(&path).unwrap().public(), key.public());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
    std::fs::write(&path, "junk").unwrap();
    assert_eq!(load_or_create_key(&path).unwrap_err().code, "bad_key");
    std::fs::remove_dir_all(dir).unwrap();
}

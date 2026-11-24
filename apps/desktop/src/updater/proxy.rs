//! The proxy for a URL, as the computer would choose it: the standard environment variables first on every OS,
//! then the system's settings (macOS: the network's proxy settings). Linux has only the environment variables. A PAC
//! file (automatic proxy configuration) isn't evaluated: the updater passes over it to the next HTTP or HTTPS entry,
//! so a PAC-only network needs `HTTPS_PROXY`. Ported from Hex's `net/proxy.rs`, which manages networks proved out
//! on (#894 there).

/// The proxy for `url`, or `None` to connect directly.
pub fn proxy_for(url: &str) -> Option<ureq::Proxy> {
    let (scheme, host) = parts(url)?;
    let proxy = from_environment(scheme, &host).or_else(|| system(url))?;
    match ureq::Proxy::new(&with_scheme(&proxy)) {
        Ok(proxy) => Some(proxy),
        Err(error) => {
            eprintln!("furiten-desktop: ignoring a proxy it can't use: {error}");
            None
        }
    }
}

/// `url` as a scheme and a host (the proxy's decision needs the scheme, and `NO_PROXY` the host alone).
fn parts(url: &str) -> Option<(&'static str, String)> {
    let (scheme, rest) = url
        .strip_prefix("https://")
        .map(|rest| ("https", rest))
        .or_else(|| url.strip_prefix("http://").map(|rest| ("http", rest)))?;
    let host = rest.split(['/', '?', '#']).next()?;
    // An authority's host is after any credentials; a trailing `:port` is not part of the name `NO_PROXY` matches.
    let host = host.rsplit_once('@').map_or(host, |(_, host)| host);
    let host = host.rsplit_once(':').filter(|(name, _)| !name.contains("::")).map_or(host, |(name, _)| name);
    let host = host.trim_end_matches('.');
    (!host.is_empty()).then(|| (scheme, host.to_owned()))
}

/// The proxy the environment names, if any: `HTTPS_PROXY` or `HTTP_PROXY` by scheme, `ALL_PROXY` for both, with
/// `NO_PROXY` exempting the host.
fn from_environment(scheme: &str, host: &str) -> Option<String> {
    let environment = |name: &str| {
        std::env::var(name)
            .ok()
            .or_else(|| std::env::var(name.to_lowercase()).ok())
            .filter(|value| !value.trim().is_empty())
    };
    let proxy = environment(if scheme == "https" { "HTTPS_PROXY" } else { "HTTP_PROXY" })
        .or_else(|| environment("ALL_PROXY"))?;
    let bypassed = environment("NO_PROXY").is_some_and(|list| no_proxy_matches(&list, host));
    (!bypassed).then_some(proxy)
}

#[cfg(target_os = "macos")]
fn system(url: &str) -> Option<String> {
    macos_proxy(url)
}

#[cfg(not(target_os = "macos"))]
fn system(_url: &str) -> Option<String> {
    None
}

/// Whether `NO_PROXY` (comma or space separated) exempts `host`: `*`, an exact host, or a domain suffix
/// (`example.com` and `.example.com` both cover `a.example.com`). A port after a name is ignored.
fn no_proxy_matches(list: &str, host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    list.split([',', ' ']).map(str::trim).filter(|entry| !entry.is_empty()).any(|entry| {
        if entry == "*" {
            return true;
        }
        let entry = entry
            .rsplit_once(':')
            .filter(|(name, port)| !name.contains(':') && port.parse::<u16>().is_ok())
            .map_or(entry, |(name, _)| name);
        let entry = entry.trim_start_matches("*.").trim_start_matches('.');
        let entry = entry.to_ascii_lowercase();
        host == entry || host.ends_with(&format!(".{entry}"))
    })
}

/// `proxy` as a URL: a bare `host:port`, as system settings write it, is an HTTP proxy.
fn with_scheme(proxy: &str) -> String {
    let proxy = proxy.trim();
    if proxy.contains("://") { proxy.to_owned() } else { format!("http://{proxy}") }
}

/// One entry of macOS' proxy list for a URL, in the order it would be tried.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) enum SystemEntry {
    /// An HTTP or HTTPS proxy, with its `host:port` when it has one.
    Http(Option<String>),
    /// Any other kind, by its type name: `kCFProxyTypeNone` (connect directly), a PAC file, SOCKS, or FTP.
    Other(String),
}

/// The first entry the updater can use: a direct entry ends the search, and kinds it can't use (a PAC file it
/// doesn't evaluate, SOCKS) are passed over for the next one, as a manual proxy listed after a PAC.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn first_usable(entries: &[SystemEntry]) -> Option<String> {
    for entry in entries {
        match entry {
            SystemEntry::Http(Some(address)) => return Some(address.clone()),
            SystemEntry::Http(None) => {}
            SystemEntry::Other(kind) if kind == "kCFProxyTypeNone" => return None,
            SystemEntry::Other(_) => {}
        }
    }
    None
}

// macOS' proxy for a URL: CFNetwork applies the network's proxy settings and their exceptions list
// (`CFNetworkCopyProxiesForURL`).
#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::{c_char, c_void};

    use super::{SystemEntry, first_usable};

    type CFTypeRef = *const c_void;

    const UTF8: u32 = 0x0800_0100;
    const SINT32: isize = 3;

    #[link(name = "CFNetwork", kind = "framework")]
    unsafe extern "C" {
        fn CFNetworkCopySystemProxySettings() -> CFTypeRef;
        fn CFNetworkCopyProxiesForURL(url: CFTypeRef, settings: CFTypeRef) -> CFTypeRef;

        static kCFProxyTypeKey: CFTypeRef;
        static kCFProxyTypeHTTP: CFTypeRef;
        static kCFProxyTypeHTTPS: CFTypeRef;
        static kCFProxyHostNameKey: CFTypeRef;
        static kCFProxyPortNumberKey: CFTypeRef;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFURLCreateWithBytes(
            allocator: CFTypeRef,
            bytes: *const u8,
            length: isize,
            encoding: u32,
            base: CFTypeRef,
        ) -> CFTypeRef;
        fn CFArrayGetCount(array: CFTypeRef) -> isize;
        fn CFArrayGetValueAtIndex(array: CFTypeRef, index: isize) -> CFTypeRef;
        fn CFDictionaryGetValue(dictionary: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
        fn CFEqual(left: CFTypeRef, right: CFTypeRef) -> u8;
        // One signature per symbol, as `accessibility` declares them.
        fn CFStringGetCString(string: CFTypeRef, buffer: *mut c_char, size: isize, encoding: u32) -> bool;
        fn CFNumberGetValue(number: CFTypeRef, kind: isize, value: *mut c_void) -> u8;
        fn CFRelease(object: CFTypeRef);
    }

    /// Releases a Core Foundation object when dropped.
    struct Owned(CFTypeRef);

    impl Drop for Owned {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: a +1 reference from a Copy or Create function.
                unsafe { CFRelease(self.0) };
            }
        }
    }

    pub(super) fn proxy(url: &str) -> Option<String> {
        // SAFETY: plain Core Foundation calls; every Copy/Create result is owned by an `Owned` and released, and
        // Get results are used only while their container lives.
        unsafe {
            let settings = Owned(CFNetworkCopySystemProxySettings());
            if settings.0.is_null() {
                return None;
            }
            let cf_url =
                Owned(CFURLCreateWithBytes(std::ptr::null(), url.as_ptr(), url.len() as isize, UTF8, std::ptr::null()));
            if cf_url.0.is_null() {
                return None;
            }
            let proxies = Owned(CFNetworkCopyProxiesForURL(cf_url.0, settings.0));
            if proxies.0.is_null() {
                return None;
            }
            let mut entries = Vec::new();
            for index in 0..CFArrayGetCount(proxies.0) {
                let entry = CFArrayGetValueAtIndex(proxies.0, index);
                let kind = CFDictionaryGetValue(entry, kCFProxyTypeKey);
                if kind.is_null() {
                    continue;
                }
                if CFEqual(kind, kCFProxyTypeHTTPS) == 0 && CFEqual(kind, kCFProxyTypeHTTP) == 0 {
                    entries.push(SystemEntry::Other(string(kind).unwrap_or_default()));
                    continue;
                }
                let host = string(CFDictionaryGetValue(entry, kCFProxyHostNameKey));
                let number = CFDictionaryGetValue(entry, kCFProxyPortNumberKey);
                let mut port: i32 = 0;
                let has_port = !number.is_null() && CFNumberGetValue(number, SINT32, (&raw mut port).cast()) != 0;
                let address = host.filter(|_| has_port).map(|host| format!("{host}:{port}"));
                entries.push(SystemEntry::Http(address));
            }
            first_usable(&entries)
        }
    }

    /// A CFString's text.
    unsafe fn string(value: CFTypeRef) -> Option<String> {
        if value.is_null() {
            return None;
        }
        let mut buffer = [0 as c_char; 512];
        // SAFETY: `value` is a CFString from the proxy dictionary; the buffer is writable for its length.
        if unsafe { !CFStringGetCString(value, buffer.as_mut_ptr(), buffer.len() as isize, UTF8) } {
            return None;
        }
        // SAFETY: `CFStringGetCString` wrote a null-terminated C string, at most the buffer's length.
        Some(unsafe { std::ffi::CStr::from_ptr(buffer.as_ptr()) }.to_string_lossy().into_owned())
    }
}

#[cfg(target_os = "macos")]
use macos::proxy as macos_proxy;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_proxy_covers_hosts_and_domains() {
        let list = "localhost, .corp.example,internal.net:8080 10.0.0.1";
        assert!(no_proxy_matches(list, "localhost"));
        assert!(no_proxy_matches(list, "a.corp.example"));
        assert!(no_proxy_matches(list, "corp.example"));
        assert!(no_proxy_matches(list, "x.internal.net"));
        assert!(no_proxy_matches(list, "10.0.0.1"));
        assert!(!no_proxy_matches(list, "huggingface.co"));
        assert!(no_proxy_matches("*", "huggingface.co"));
    }

    #[test]
    fn system_proxy_lists_pass_over_a_pac_entry() {
        let pac = || SystemEntry::Other("kCFProxyTypeAutoConfigurationURL".into());
        let direct = || SystemEntry::Other("kCFProxyTypeNone".into());
        assert_eq!(
            first_usable(&[pac(), SystemEntry::Http(Some("127.0.0.1:8080".into()))]).as_deref(),
            Some("127.0.0.1:8080")
        );
        assert_eq!(first_usable(&[pac()]), None);
        assert_eq!(first_usable(&[direct(), SystemEntry::Http(Some("p:1".into()))]), None);
        assert_eq!(
            first_usable(&[
                SystemEntry::Http(None),
                SystemEntry::Other("kCFProxyTypeSOCKS".into()),
                SystemEntry::Http(Some("p:3128".into())),
            ])
            .as_deref(),
            Some("p:3128")
        );
    }

    #[test]
    fn bare_addresses_get_a_scheme() {
        assert_eq!(with_scheme("proxy:8080"), "http://proxy:8080");
        assert_eq!(with_scheme(" socks5://p:1080 "), "socks5://p:1080");
    }

    #[test]
    fn urls_parse_into_scheme_and_host() {
        assert_eq!(parts("https://anoma.ly/release/furiten/desktop/stable.json"), Some(("https", "anoma.ly".into())));
        assert_eq!(parts("http://proxy.example:8080/x"), Some(("http", "proxy.example".into())));
        assert_eq!(parts("https://user:pass@host.example/y"), Some(("https", "host.example".into())));
        assert_eq!(parts("ftp://example.com/"), None);
        assert_eq!(parts("anoma.ly/release"), None);
    }
}

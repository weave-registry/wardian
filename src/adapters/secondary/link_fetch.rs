//! Fetching a zip from a link the user pasted, safely: the server never fetches its own or the
//! private network's addresses for a user.

use crate::ports::web::Downloader;
use std::{
    io::Read,
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    time::Duration,
};

pub struct LinkFetcher {
    /// Allow the local network too (IMPORT_ALLOW_LAN=1).
    allow_lan: bool,
}

impl LinkFetcher {
    pub fn new(allow_lan: bool) -> LinkFetcher {
        LinkFetcher { allow_lan }
    }
}

impl Downloader for LinkFetcher {
    fn fetch(&self, url: &str, limit: u64) -> Result<Vec<u8>, String> {
        let allow_lan = self.allow_lan;
        let resp = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(60))
            .resolver(move |netloc: &str| public_only(netloc, allow_lan))
            .build()
            .get(url)
            .call()
            .map_err(|e| format!("download failed: {e}"))?;
        let mut buf = Vec::new();
        resp.into_reader().take(limit + 1).read_to_end(&mut buf).map_err(|e| format!("download failed: {e}"))?;
        Ok(buf)
    }
}

/// Resolves a host for a link import and drops every address the server
/// should never fetch for a user: itself, cloud metadata (169.254.169.254),
/// and, unless `allow_lan`, the private network. ureq resolves through this
/// for every connection, redirects included, so a link cannot reach an
/// internal address by redirecting or by re-resolving to a new one.
fn public_only(netloc: &str, allow_lan: bool) -> std::io::Result<Vec<SocketAddr>> {
    let addrs: Vec<SocketAddr> = netloc.to_socket_addrs()?.collect();
    let ok: Vec<SocketAddr> = addrs.iter().copied().filter(|a| fetchable(a.ip(), allow_lan)).collect();
    if ok.is_empty() && !addrs.is_empty() {
        let hint = if allow_lan { "" } else { " (set IMPORT_ALLOW_LAN=1 to allow the local network)" };
        return Err(std::io::Error::other(format!("links to internal addresses are blocked{hint}")));
    }
    Ok(ok)
}

fn fetchable(ip: IpAddr, allow_lan: bool) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            let never = v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_multicast()
                || v4.is_broadcast()
                || a == 0
                || (a == 100 && (64..128).contains(&b)); // carrier-grade NAT
            !never && (allow_lan || !v4.is_private())
        }
        IpAddr::V6(v6) => {
            // An IPv4 address written as IPv6 (::ffff:127.0.0.1) gets the IPv4 rules.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return fetchable(IpAddr::V4(v4), allow_lan);
            }
            let first = v6.segments()[0];
            let never = v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (first & 0xffc0) == 0xfe80; // link-local
            let lan = (first & 0xfe00) == 0xfc00; // unique local
            !never && (allow_lan || !lan)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fetchable;

    #[test]
    fn blocks_internal_addresses() {
        for ip in ["127.0.0.1", "169.254.169.254", "0.0.0.0", "100.64.0.1", "::1", "fe80::1", "::ffff:127.0.0.1", "::ffff:169.254.169.254"] {
            assert!(!fetchable(ip.parse().unwrap(), true), "{ip} must always be blocked");
        }
        for ip in ["10.0.0.5", "192.168.1.10", "172.16.0.1", "fd00::1"] {
            assert!(!fetchable(ip.parse().unwrap(), false), "{ip} blocked by default");
            assert!(fetchable(ip.parse().unwrap(), true), "{ip} allowed with IMPORT_ALLOW_LAN");
        }
        for ip in ["8.8.8.8", "142.250.80.46", "2607:f8b0::1"] {
            assert!(fetchable(ip.parse().unwrap(), false), "{ip} is public");
        }
    }
}

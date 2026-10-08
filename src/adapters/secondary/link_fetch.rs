//! Fetching a zip from a link the user pasted, safely: the server never fetches its own or the
//! private network's addresses for a user.

use crate::ports::web::Downloader;
use std::{
    io::Read,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs},
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
            // An IPv4 address written as IPv6 gets the IPv4 rules: mapped (::ffff:127.0.0.1),
            // compatible (::127.0.0.1, which also covers :: and ::1) and NAT64 (64:ff9b::127.0.0.1).
            if let Some(v4) = v6.to_ipv4().or_else(|| nat64(v6)) {
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

/// The IPv4 address inside a NAT64 address of the well-known prefix 64:ff9b::/96 (RFC 6052), which a
/// NAT64 gateway forwards to that IPv4 address.
fn nat64(v6: Ipv6Addr) -> Option<Ipv4Addr> {
    let o = v6.octets();
    (v6.segments()[..6] == [0x64, 0xff9b, 0, 0, 0, 0]).then(|| Ipv4Addr::new(o[12], o[13], o[14], o[15]))
}

#[cfg(test)]
mod tests {
    use super::{fetchable, public_only};

    /// security.md Import links (#149): multicast, broadcast and `localhost` are never fetched.
    #[test]
    fn claim_multicast_broadcast_and_localhost_are_never_fetched() {
        for ip in ["224.0.0.1", "239.255.255.250", "255.255.255.255", "ff02::1"] {
            assert!(!fetchable(ip.parse().unwrap(), true), "{ip} must always be blocked");
        }
        for allow_lan in [false, true] {
            let e = public_only("localhost:80", allow_lan).unwrap_err();
            assert!(e.to_string().contains("internal addresses are blocked"), "{e}");
        }
    }

    /// security.md Import links, ADR-2610081041 (#149, C9): an IPv4 address written as IPv6, in
    /// every form, gets the IPv4 rules: mapped (::ffff:a.b.c.d), compatible (::a.b.c.d) and NAT64
    /// (64:ff9b::a.b.c.d).
    #[test]
    fn claim_ipv4_written_as_ipv6_gets_the_ipv4_rules() {
        for prefix in ["::ffff:", "::", "64:ff9b::"] {
            for v4 in ["127.0.0.1", "169.254.169.254", "10.0.0.5", "224.0.0.1", "255.255.255.255", "100.64.0.1"] {
                let ip = format!("{prefix}{v4}");
                assert!(!fetchable(ip.parse().unwrap(), false), "{ip} must be blocked as {v4} is");
            }
            for v4 in ["127.0.0.1", "169.254.169.254"] {
                let ip = format!("{prefix}{v4}");
                assert!(!fetchable(ip.parse().unwrap(), true), "{ip} must be blocked even with IMPORT_ALLOW_LAN");
            }
            let lan = format!("{prefix}192.168.1.10");
            assert!(fetchable(lan.parse().unwrap(), true), "{lan} is allowed with IMPORT_ALLOW_LAN, as its IPv4 address is");
            let public = format!("{prefix}8.8.8.8");
            assert!(fetchable(public.parse().unwrap(), false), "{public} is public");
        }
    }

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

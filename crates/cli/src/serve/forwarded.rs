//! What a proxy in front of Phera says about the original request.
//!
//! Behind a load balancer every connection comes from the balancer, over
//! plain HTTP. The balancer passes on what it saw:
//!
//! ```text
//! X-Forwarded-For: 203.0.113.7, 10.0.0.4     the client, then proxies it passed
//! X-Forwarded-Proto: https
//! X-Forwarded-Host: shop.example
//! ```
//!
//! Any client can send these headers too, so they are believed only when
//! the connection itself comes from an address listed with
//! `--trusted-proxies`. Then `REMOTE_ADDR`, `HTTPS`, `SERVER_PORT` and
//! `HTTP_HOST` describe the original request, as they would without a proxy.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// A range of addresses such as `10.0.0.0/8`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AddressRange {
    network: IpAddr,
    prefix_bits: u8,
}

impl AddressRange {
    const fn v4(a: u8, b: u8, c: u8, d: u8, prefix_bits: u8) -> AddressRange {
        AddressRange {
            network: IpAddr::V4(Ipv4Addr::new(a, b, c, d)),
            prefix_bits,
        }
    }

    const fn v6(first: u16, prefix_bits: u8) -> AddressRange {
        AddressRange {
            network: IpAddr::V6(Ipv6Addr::new(first, 0, 0, 0, 0, 0, 0, 0)),
            prefix_bits,
        }
    }

    /// The addresses no public client can have: loopback, the private
    /// networks and link-local. A proxy on the same host or in the same
    /// cluster always comes from one of them.
    const PRIVATE: [AddressRange; 8] = [
        AddressRange::v4(127, 0, 0, 0, 8),
        AddressRange::v4(10, 0, 0, 0, 8),
        AddressRange::v4(172, 16, 0, 0, 12),
        AddressRange::v4(192, 168, 0, 0, 16),
        AddressRange::v4(169, 254, 0, 0, 16),
        AddressRange {
            network: IpAddr::V6(Ipv6Addr::LOCALHOST),
            prefix_bits: 128,
        },
        AddressRange::v6(0xfc00, 7),
        AddressRange::v6(0xfe80, 10),
    ];

    /// Parses `10.0.0.0/8`, `2001:db8::/32`, or one address.
    fn parse(text: &str) -> Option<AddressRange> {
        let (address, prefix) = match text.split_once('/') {
            Some((address, prefix)) => (address, Some(prefix)),
            None => (text, None),
        };
        let network: IpAddr = address.trim().parse().ok()?;
        let widest = if network.is_ipv4() { 32 } else { 128 };
        let prefix_bits = match prefix {
            Some(prefix) => prefix.trim().parse().ok().filter(|bits| *bits <= widest)?,
            None => widest,
        };
        Some(AddressRange {
            network,
            prefix_bits,
        })
    }

    fn contains(&self, address: IpAddr) -> bool {
        match (self.network, plain(address)) {
            (IpAddr::V4(network), IpAddr::V4(address)) => same_prefix(
                network.to_bits().into(),
                address.to_bits().into(),
                32,
                self.prefix_bits,
            ),
            (IpAddr::V6(network), IpAddr::V6(address)) => {
                same_prefix(network.to_bits(), address.to_bits(), 128, self.prefix_bits)
            }
            _ => false,
        }
    }
}

/// `::ffff:10.0.0.4` is the IPv4 address 10.0.0.4 seen through an IPv6 socket.
fn plain(address: IpAddr) -> IpAddr {
    match address {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(address, IpAddr::V4),
        IpAddr::V4(_) => address,
    }
}

fn same_prefix(left: u128, right: u128, width: u32, prefix_bits: u8) -> bool {
    let ignored = width - u32::from(prefix_bits);
    // Shifting a number by its whole width is not defined, and a prefix of
    // zero bits matches everything.
    ignored >= width || (left >> ignored) == (right >> ignored)
}

/// The proxies whose forwarding headers are believed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TrustedProxies {
    ranges: Vec<AddressRange>,
}

impl TrustedProxies {
    /// Parses the value of `--trusted-proxies`: ranges or addresses
    /// separated by commas, where `private` stands for every private range.
    pub(crate) fn parse(list: &str) -> Result<TrustedProxies, String> {
        let mut ranges = Vec::new();
        for item in list
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            if item.eq_ignore_ascii_case("private") {
                ranges.extend(AddressRange::PRIVATE);
            } else {
                let range = AddressRange::parse(item)
                    .ok_or_else(|| format!("Not an address or range like 10.0.0.0/8: {item}"))?;
                ranges.push(range);
            }
        }
        Ok(TrustedProxies { ranges })
    }

    fn trusts(&self, address: IpAddr) -> bool {
        self.ranges.iter().any(|range| range.contains(address))
    }

    /// What the forwarding headers of a request say, if the connection
    /// comes from a trusted proxy.
    pub(crate) fn original_request(
        &self,
        peer: IpAddr,
        headers: &[(String, Vec<u8>)],
    ) -> Option<Original> {
        if !self.trusts(peer) {
            return None;
        }
        let header = |name: &str| joined(headers, name);
        // The standard `Forwarded` header carries the same facts; proxies
        // send one form or the other, and the older one is far more common.
        let standard = header("forwarded").map(|value| StandardForwarded::parse(&value));
        let standard = standard.unwrap_or_default();
        let chain = header("x-forwarded-for")
            .map(|value| value.split(',').map(str::to_owned).collect())
            .unwrap_or(standard.chain);
        let scheme = header("x-forwarded-proto")
            .and_then(|value| first_item(&value))
            .or(standard.scheme);
        Some(Original {
            client: self.client_in(&chain),
            https: scheme.map(|scheme| scheme.eq_ignore_ascii_case("https")),
            host: header("x-forwarded-host")
                .and_then(|value| first_item(&value))
                .or(standard.host),
            port: header("x-forwarded-port")
                .and_then(|value| first_item(&value))
                .and_then(|port| port.parse().ok()),
        })
    }

    /// The last address in the chain that is not one of our own proxies:
    /// everything to its left was written by someone we do not know.
    fn client_in(&self, chain: &[String]) -> Option<IpAddr> {
        let mut client = None;
        for entry in chain.iter().rev() {
            let address = address_of(entry)?;
            client = Some(address);
            if !self.trusts(address) {
                break;
            }
        }
        client
    }
}

/// The original request as the proxy described it. A part the proxy did
/// not mention is `None` and stays as the connection shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Original {
    pub(crate) client: Option<IpAddr>,
    pub(crate) https: Option<bool>,
    pub(crate) host: Option<String>,
    pub(crate) port: Option<u16>,
}

impl Original {
    /// The port the client connected to: the one the proxy named, or the
    /// usual one for the scheme it named.
    pub(crate) fn server_port(&self) -> Option<u16> {
        self.port
            .or(self.https.map(|https| if https { 443 } else { 80 }))
    }

    /// The client's address, with no port: the proxy does not pass it on.
    pub(crate) fn client_address(&self) -> Option<SocketAddr> {
        self.client.map(|client| SocketAddr::new(client, 0))
    }
}

/// `Forwarded: for=203.0.113.7;proto=https;host=shop.example, for=10.0.0.4`
#[derive(Default)]
struct StandardForwarded {
    chain: Vec<String>,
    scheme: Option<String>,
    host: Option<String>,
}

impl StandardForwarded {
    fn parse(value: &str) -> StandardForwarded {
        let mut parsed = StandardForwarded::default();
        for hop in value.split(',') {
            for pair in hop.split(';') {
                let Some((name, value)) = pair.split_once('=') else {
                    continue;
                };
                let value = value.trim().trim_matches('"').to_owned();
                match name.trim().to_ascii_lowercase().as_str() {
                    "for" => parsed.chain.push(value),
                    // As with the X- headers, the first hop's word counts.
                    "proto" if parsed.scheme.is_none() => parsed.scheme = Some(value),
                    "host" if parsed.host.is_none() => parsed.host = Some(value),
                    _ => {}
                }
            }
        }
        parsed
    }
}

/// Every value of a header, joined with commas, or `None` if it is absent.
fn joined(headers: &[(String, Vec<u8>)], name: &str) -> Option<String> {
    let values: Vec<_> = headers
        .iter()
        .filter(|(header, _)| header.eq_ignore_ascii_case(name))
        .map(|(_, value)| String::from_utf8_lossy(value))
        .collect();
    (!values.is_empty()).then(|| values.join(","))
}

fn first_item(value: &str) -> Option<String> {
    let first = value.split(',').next()?.trim();
    (!first.is_empty()).then(|| first.to_owned())
}

/// The address in `203.0.113.7`, `203.0.113.7:4711`, `[2001:db8::1]:4711`
/// or `"[2001:db8::1]"`.
fn address_of(entry: &str) -> Option<IpAddr> {
    let entry = entry.trim().trim_matches('"');
    if let Ok(address) = entry.parse::<IpAddr>() {
        return Some(address);
    }
    if let Ok(with_port) = entry.parse::<SocketAddr>() {
        return Some(with_port.ip());
    }
    entry
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .and_then(|inner| inner.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> Vec<(String, Vec<u8>)> {
        pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.as_bytes().to_vec()))
            .collect()
    }

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    #[test]
    fn ranges_match_by_prefix() {
        let range = AddressRange::parse("172.16.0.0/12").unwrap();
        assert!(range.contains(ip("172.16.0.1")) && range.contains(ip("172.31.255.255")));
        assert!(!range.contains(ip("172.32.0.0")) && !range.contains(ip("::1")));
        assert!(
            AddressRange::parse("10.1.2.3")
                .unwrap()
                .contains(ip("10.1.2.3"))
        );
        assert!(
            !AddressRange::parse("10.1.2.3")
                .unwrap()
                .contains(ip("10.1.2.4"))
        );
        assert!(
            AddressRange::parse("0.0.0.0/0")
                .unwrap()
                .contains(ip("8.8.8.8"))
        );
        assert!(
            AddressRange::parse("2001:db8::/32")
                .unwrap()
                .contains(ip("2001:db8:1::5"))
        );
        // An IPv4 client seen through an IPv6 socket.
        assert!(
            AddressRange::parse("10.0.0.0/8")
                .unwrap()
                .contains(ip("::ffff:10.2.3.4"))
        );
        for bad in ["10.0.0.0/33", "::/129", "ten", "10.0.0.0/x", ""] {
            assert_eq!(AddressRange::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_word_private_covers_the_private_networks() {
        let proxies = TrustedProxies::parse("private").unwrap();
        for inside in [
            "127.0.0.1",
            "10.9.8.7",
            "172.20.1.1",
            "192.168.1.1",
            "::1",
            "fd00::1",
        ] {
            assert!(proxies.trusts(ip(inside)), "{inside}");
        }
        for outside in ["8.8.8.8", "172.32.0.1", "2001:db8::1"] {
            assert!(!proxies.trusts(ip(outside)), "{outside}");
        }
        assert!(TrustedProxies::parse("10.0.0.0/8, nonsense").is_err());
        assert_eq!(
            TrustedProxies::parse("").unwrap(),
            TrustedProxies::default()
        );
    }

    #[test]
    fn headers_from_a_stranger_are_ignored() {
        let proxies = TrustedProxies::parse("10.0.0.0/8").unwrap();
        let sent = headers(&[
            ("x-forwarded-for", "1.2.3.4"),
            ("x-forwarded-proto", "https"),
        ]);
        assert_eq!(proxies.original_request(ip("203.0.113.9"), &sent), None);
        assert_eq!(
            TrustedProxies::default().original_request(ip("10.0.0.1"), &sent),
            None
        );
    }

    #[test]
    fn a_trusted_proxy_describes_the_original_request() {
        let proxies = TrustedProxies::parse("10.0.0.0/8").unwrap();
        let sent = headers(&[
            ("x-forwarded-for", "203.0.113.7, 10.0.0.4"),
            ("x-forwarded-proto", "https"),
            ("x-forwarded-host", "shop.example"),
        ]);
        let original = proxies.original_request(ip("10.0.0.5"), &sent).unwrap();
        assert_eq!(original.client, Some(ip("203.0.113.7")));
        assert_eq!(original.https, Some(true));
        assert_eq!(original.host.as_deref(), Some("shop.example"));
        assert_eq!(original.server_port(), Some(443));

        let with_port = headers(&[("x-forwarded-proto", "http"), ("x-forwarded-port", "8443")]);
        let original = proxies
            .original_request(ip("10.0.0.5"), &with_port)
            .unwrap();
        assert_eq!(
            (original.https, original.server_port()),
            (Some(false), Some(8443))
        );
        assert_eq!(original.client, None);
    }

    /// A client that writes its own `X-Forwarded-For` only adds to the left
    /// of what the proxy appends.
    #[test]
    fn the_client_is_the_last_address_that_is_not_a_proxy() {
        let proxies = TrustedProxies::parse("10.0.0.0/8").unwrap();
        let client = |chain: &str| {
            let sent = headers(&[("x-forwarded-for", chain)]);
            proxies
                .original_request(ip("10.0.0.5"), &sent)
                .unwrap()
                .client
        };
        assert_eq!(
            client("9.9.9.9, 203.0.113.7, 10.0.0.4"),
            Some(ip("203.0.113.7"))
        );
        assert_eq!(client("203.0.113.7:4711"), Some(ip("203.0.113.7")));
        assert_eq!(
            client("[2001:db8::1]:4711, 10.0.0.4"),
            Some(ip("2001:db8::1"))
        );
        // Only proxies in the chain: the first of them is the client.
        assert_eq!(client("10.0.0.9, 10.0.0.4"), Some(ip("10.0.0.9")));
        // Something unreadable where the client should be: nothing is claimed.
        assert_eq!(client("unknown, 10.0.0.4"), None);
        assert_eq!(client(""), None);
    }

    #[test]
    fn the_standard_forwarded_header_is_understood() {
        let proxies = TrustedProxies::parse("private").unwrap();
        let sent = headers(&[(
            "forwarded",
            "for=\"[2001:db8::7]:4711\";proto=https;host=shop.example, for=10.0.0.4",
        )]);
        let original = proxies.original_request(ip("127.0.0.1"), &sent).unwrap();
        assert_eq!(original.client, Some(ip("2001:db8::7")));
        assert_eq!(original.https, Some(true));
        assert_eq!(original.host.as_deref(), Some("shop.example"));
    }
}

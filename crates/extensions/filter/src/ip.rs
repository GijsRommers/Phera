//! IP validation classifies special-purpose ranges separately from address syntax.
//!
//! ```php
//! filter_var('::ffff:8.8.8.8', FILTER_VALIDATE_IP, FILTER_FLAG_NO_RES_RANGE); // false
//! filter_var('192.168.0.1', FILTER_VALIDATE_IP, FILTER_FLAG_NO_PRIV_RANGE); // false
//! ```

use std::net::{IpAddr, Ipv6Addr};

use crate::constants::*;
use crate::options::Flags;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Range {
    Public,
    Private,
    Reserved,
    Special,
}

pub fn valid(input: &[u8], flags: Flags) -> bool {
    let Some(ip) = std::str::from_utf8(input)
        .ok()
        .and_then(|text| text.parse::<IpAddr>().ok())
    else {
        return false;
    };
    let permits_both = flags.has(IPV4) == flags.has(IPV6);
    let range = match ip {
        IpAddr::V4(address) if permits_both || flags.has(IPV4) => ipv4(address.octets()),
        IpAddr::V6(address) if permits_both || flags.has(IPV6) => {
            // Every IPv6 spelling with a dotted suffix is classified as IPv4-mapped.
            if input.contains(&b'.') {
                Range::Reserved
            } else {
                ipv6(address.segments())
            }
        }
        _ => return false,
    };

    !(flags.has(GLOBAL_RANGE) && range != Range::Public
        || flags.has(NO_PRIV_RANGE) && range == Range::Private
        || flags.has(NO_RES_RANGE) && range == Range::Reserved)
}

pub fn ipv6_literal(input: &[u8]) -> bool {
    std::str::from_utf8(input).is_ok_and(|text| text.parse::<Ipv6Addr>().is_ok())
}

fn ipv4(address: [u8; 4]) -> Range {
    match address {
        [0 | 127 | 240..=255, ..] | [169, 254, ..] => Range::Reserved,
        [10, ..] | [172, 16..=31, ..] | [192, 168, ..] => Range::Private,
        [100, 64..=127, ..]
        | [192, 0, 0 | 2, _]
        | [198, 18 | 19, ..]
        | [198, 51, 100, _]
        | [203, 0, 113, _] => Range::Special,
        _ => Range::Public,
    }
}

fn ipv6(address: [u16; 8]) -> Range {
    match address {
        [0, 0, 0, 0, 0, 0, 0, 0 | 1] | [0, 0, 0, 0, 0, 0xffff, ..] | [0xfe80..=0xfebf, ..] => {
            Range::Reserved
        }
        [0xfc00..=0xfdff, ..] => Range::Private,
        [0x0100, 0, 0, 0, ..] | [0x2001, 0..=0x01ff | 0x0db8, ..] | [0x2002, ..] => Range::Special,
        _ => Range::Public,
    }
}

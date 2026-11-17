//! PHP's Murmur3 byte order and 64-bit seed: `hash('murmur3f', $s, false, ['seed'=>-1])`.
//! Ported from php-src/ext/hash/murmur/PMurHash{,128}.c (public domain).
fn mix32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x85ebca6b);
    x ^= x >> 13;
    x = x.wrapping_mul(0xc2b2ae35);
    x ^ (x >> 16)
}
fn mix64(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ceb9fe1a85ec53);
    x ^ (x >> 33)
}
fn word(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .enumerate()
        .fold(0, |v, (i, b)| v | (u64::from(*b) << (i * 8)))
}
pub fn a(input: &[u8], seed: u32) -> Vec<u8> {
    let mut h = seed;
    for bytes in input.chunks(4) {
        let k = (word(bytes) as u32)
            .wrapping_mul(0xcc9e2d51)
            .rotate_left(15)
            .wrapping_mul(0x1b873593);
        h ^= k;
        if bytes.len() == 4 {
            h = h.rotate_left(13).wrapping_mul(5).wrapping_add(0xe6546b64);
        }
    }
    mix32(h ^ input.len() as u32).to_be_bytes().to_vec()
}
pub fn c(input: &[u8], seed: u32) -> Vec<u8> {
    let mut h = [seed; 4];
    let constants = [0x239b961bu32, 0xab0e9789, 0x38b34ae5, 0xa1e38b93];
    let rotates = [19, 17, 15, 13];
    let adds = [0x561ccd1b, 0x0bcaa747, 0x96cd1c35, 0x32ac3b17];
    for block in input.chunks(16) {
        for (i, bytes) in block.chunks(4).enumerate() {
            h[i] ^= (word(bytes) as u32)
                .wrapping_mul(constants[i])
                .rotate_left(15 + i as u32)
                .wrapping_mul(constants[(i + 1) % 4]);
            if block.len() == 16 {
                h[i] = h[i]
                    .rotate_left(rotates[i])
                    .wrapping_add(h[(i + 1) % 4])
                    .wrapping_mul(5)
                    .wrapping_add(adds[i]);
            }
        }
    }
    for x in &mut h {
        *x ^= input.len() as u32;
    }
    sum32(&mut h);
    h = h.map(mix32);
    sum32(&mut h);
    h.into_iter().flat_map(u32::to_be_bytes).collect()
}
fn sum32(h: &mut [u32; 4]) {
    h[0] = h[0]
        .wrapping_add(h[1])
        .wrapping_add(h[2])
        .wrapping_add(h[3]);
    for i in 1..4 {
        h[i] = h[i].wrapping_add(h[0]);
    }
}
pub fn f(input: &[u8], seed: u64) -> Vec<u8> {
    let (mut a, mut b) = (seed, seed);
    for block in input.chunks(16) {
        if block.len() > 8 {
            b ^= word(&block[8..])
                .wrapping_mul(0x4cf5ad432745937f)
                .rotate_left(33)
                .wrapping_mul(0x87c37b91114253d5);
        }
        a ^= word(&block[..block.len().min(8)])
            .wrapping_mul(0x87c37b91114253d5)
            .rotate_left(31)
            .wrapping_mul(0x4cf5ad432745937f);
        if block.len() == 16 {
            // The first accumulator uses the previous second accumulator.
            let old_b = b ^ word(&block[8..])
                .wrapping_mul(0x4cf5ad432745937f)
                .rotate_left(33)
                .wrapping_mul(0x87c37b91114253d5);
            a = a
                .rotate_left(27)
                .wrapping_add(old_b)
                .wrapping_mul(5)
                .wrapping_add(0x52dce729);
            b = b
                .rotate_left(31)
                .wrapping_add(a)
                .wrapping_mul(5)
                .wrapping_add(0x38495ab5);
        }
    }
    a ^= input.len() as u64;
    b ^= input.len() as u64;
    a = a.wrapping_add(b);
    b = b.wrapping_add(a);
    a = mix64(a);
    b = mix64(b);
    a = a.wrapping_add(b);
    b = b.wrapping_add(a);
    [a, b].into_iter().flat_map(u64::to_be_bytes).collect()
}

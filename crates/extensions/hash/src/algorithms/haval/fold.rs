//! HAVAL output folding from php-src/ext/hash/hash_haval.c.
//! Copyright The PHP Group; BSD-3-Clause, see LICENSE-PHP.
pub fn fold(s: &mut [u32; 8], bits: usize) {
    match bits {
        128 => {
            s[3] = s[3].wrapping_add(
                (s[7] & 0xFF000000)
                    | (s[6] & 0x00FF0000)
                    | (s[5] & 0x0000FF00)
                    | (s[4] & 0x000000FF),
            );
            s[2] = s[2].wrapping_add(
                (((s[7] & 0x00FF0000) | (s[6] & 0x0000FF00) | (s[5] & 0x000000FF)) << 8)
                    | ((s[4] & 0xFF000000) >> 24),
            );
            s[1] = s[1].wrapping_add(
                (((s[7] & 0x0000FF00) | (s[6] & 0x000000FF)) << 16)
                    | (((s[5] & 0xFF000000) | (s[4] & 0x00FF0000)) >> 16),
            );
            s[0] = s[0].wrapping_add(
                ((s[7] & 0x000000FF) << 24)
                    | (((s[6] & 0xFF000000) | (s[5] & 0x00FF0000) | (s[4] & 0x0000FF00)) >> 8),
            );
        }
        160 => {
            s[4] = s[4].wrapping_add(
                ((s[7] & 0xFE000000) | (s[6] & 0x01F80000) | (s[5] & 0x0007F000)) >> 12,
            );
            s[3] = s[3].wrapping_add(
                ((s[7] & 0x01F80000) | (s[6] & 0x0007F000) | (s[5] & 0x00000FC0)) >> 6,
            );
            s[2] =
                s[2].wrapping_add((s[7] & 0x0007F000) | (s[6] & 0x00000FC0) | (s[5] & 0x0000003F));
            s[1] = s[1].wrapping_add(
                ((s[7] & 0x00000FC0) | (s[6] & 0x0000003F) | (s[5] & 0xFE000000)).rotate_right(25),
            );
            s[0] = s[0].wrapping_add(
                ((s[7] & 0x0000003F) | (s[6] & 0xFE000000) | (s[5] & 0x01F80000)).rotate_right(19),
            );
        }
        192 => {
            s[5] = s[5].wrapping_add(((s[7] & 0xFC000000) | (s[6] & 0x03E00000)) >> 21);
            s[4] = s[4].wrapping_add(((s[7] & 0x03E00000) | (s[6] & 0x001F0000)) >> 16);
            s[3] = s[3].wrapping_add(((s[7] & 0x001F0000) | (s[6] & 0x0000FC00)) >> 10);
            s[2] = s[2].wrapping_add(((s[7] & 0x0000FC00) | (s[6] & 0x000003E0)) >> 5);
            s[1] = s[1].wrapping_add((s[7] & 0x000003E0) | (s[6] & 0x0000001F));
            s[0] = s[0].wrapping_add(((s[7] & 0x0000001F) | (s[6] & 0xFC000000)).rotate_right(26));
        }
        224 => {
            s[6] = s[6].wrapping_add(s[7] & 0x0000000F);
            s[5] = s[5].wrapping_add((s[7] >> 4) & 0x0000001F);
            s[4] = s[4].wrapping_add((s[7] >> 9) & 0x0000000F);
            s[3] = s[3].wrapping_add((s[7] >> 13) & 0x0000001F);
            s[2] = s[2].wrapping_add((s[7] >> 18) & 0x0000000F);
            s[1] = s[1].wrapping_add((s[7] >> 22) & 0x0000001F);
            s[0] = s[0].wrapping_add((s[7] >> 27) & 0x0000001F);
        }
        _ => {}
    }
}

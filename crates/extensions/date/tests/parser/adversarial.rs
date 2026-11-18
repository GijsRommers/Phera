//! Deterministic prefix, conflict and corruption cases exercise longest-match recovery.
pub fn inputs() -> Vec<Vec<u8>> {
    let mut strings = Vec::new();
    for month in [
        "January", "Feb", "March", "Apr", "May", "Jun", "July", "Aug", "Sept", "Oct", "Nov", "Dec",
        "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII",
    ] {
        for sep in ["", " ", "-", ".", "\t", "/"] {
            for year in ["0", "69", "70", "100", "2000"] {
                for day in ["0", "1st", "29", "31", "32"] {
                    for input in [
                        format!("{day}{sep}{month}{sep}{year}"),
                        format!("{month}{sep}{day}, {year}"),
                        format!("{year}{sep}{month}{sep}{day}"),
                    ] {
                        strings.push(input.into_bytes());
                    }
                }
            }
        }
    }
    for year in [
        "-9999999",
        "+9223372036854775807",
        "+9223372036854775808",
        "-9223372036854775808",
        "-9223372036854775809",
        "+9999999999999999999",
        "+0000000000000000001",
    ] {
        strings.push(format!("{year}-01-01").into_bytes());
    }
    for zeros in [18, 19, 23, 24, 25, 100] {
        strings.push(format!("@{}1.5", "0".repeat(zeros)).into_bytes());
    }
    let words = super::examples();
    let mut seed = 0x55310d3a99u64;
    for _ in 0..12000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let mut input = words[seed as usize % words.len()].as_bytes().to_vec();
        let position = (seed >> 16) as usize % (input.len() + 1);
        let alphabet = b"0123456789-+/:.\t\r\n\0 @(),ABCxyz\xff";
        let byte = alphabet[(seed >> 32) as usize % alphabet.len()];
        match seed % 4 {
            0 => input.insert(position, byte),
            1 if position < input.len() => {
                input.remove(position);
            }
            2 => input.extend_from_slice(words[(seed >> 32) as usize % words.len()].as_bytes()),
            _ if position < input.len() => input[position] = byte,
            _ => {}
        }
        strings.push(input);
    }
    strings.sort();
    strings.dedup();
    strings
}

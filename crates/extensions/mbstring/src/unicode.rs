//! PHP 8.5 uses Unicode 17: `mb_convert_case('ß', MB_CASE_UPPER) === 'SS'`.
use crate::encoding::read_u32;

const TABLES: [&[u8]; 8] = [
    include_bytes!("../data/case-0.bin"),
    include_bytes!("../data/case-1.bin"),
    include_bytes!("../data/case-2.bin"),
    include_bytes!("../data/case-3.bin"),
    include_bytes!("../data/case-4.bin"),
    include_bytes!("../data/case-5.bin"),
    include_bytes!("../data/case-6.bin"),
    include_bytes!("../data/case-7.bin"),
];
#[derive(Clone, Copy)]
pub enum Case {
    Upper,
    Lower,
    Title,
    Fold,
    UpperSimple,
    LowerSimple,
    TitleSimple,
    FoldSimple,
}
impl Case {
    pub fn parse(mode: i64) -> Option<Self> {
        match mode {
            0 => Some(Self::Upper),
            1 => Some(Self::Lower),
            2 => Some(Self::Title),
            3 => Some(Self::Fold),
            4 => Some(Self::UpperSimple),
            5 => Some(Self::LowerSimple),
            6 => Some(Self::TitleSimple),
            7 => Some(Self::FoldSimple),
            _ => None,
        }
    }
}
fn lookup(table: &[u8], size: usize, point: u32) -> Option<&[u8]> {
    let index = table.chunks_exact(size).len();
    let mut low = 0;
    let mut high = index;
    while low < high {
        let mid = low + (high - low) / 2;
        let record = &table[mid * size..][..size];
        match read_u32(record).cmp(&point) {
            std::cmp::Ordering::Equal => return Some(record),
            std::cmp::Ordering::Less => low = mid + 1,
            std::cmp::Ordering::Greater => high = mid,
        }
    }
    None
}
pub fn width(point: u32) -> usize {
    1 + usize::from(lookup(include_bytes!("../data/width.bin"), 4, point).is_some())
}
fn properties(point: u32) -> u8 {
    lookup(include_bytes!("../data/properties.bin"), 5, point).map_or(0, |r| r[4])
}
pub fn map_one(point: u32, mode: Case, out: &mut Vec<u32>) {
    static MAPS: [std::sync::OnceLock<Vec<[u32; 3]>>; 8] =
        [const { std::sync::OnceLock::new() }; 8];
    let table = MAPS[mode as usize].get_or_init(|| {
        let mut table = vec![[u32::MAX; 3]; 0x20000];
        for record in TABLES[mode as usize].as_chunks::<16>().0.iter() {
            let cp = read_u32(record) as usize;
            if cp < table.len() {
                table[cp] = [
                    read_u32(&record[4..]),
                    read_u32(&record[8..]),
                    read_u32(&record[12..]),
                ];
            }
        }
        table
    });
    if let Some(mapped) = table.get(point as usize).filter(|m| m[0] != u32::MAX) {
        out.extend(mapped.iter().copied().take_while(|cp| *cp != u32::MAX));
    } else {
        out.push(point);
    }
}

pub fn convert(points: &[u32], mode: Case) -> Vec<u32> {
    let mut out = Vec::with_capacity(points.len());
    let mut in_word = false;
    for (i, &point) in points.iter().enumerate() {
        let effective = match mode {
            Case::Title if in_word => Case::Lower,
            Case::TitleSimple if in_word => Case::LowerSimple,
            other => other,
        };
        if point == 0x3a3 && matches!(effective, Case::Lower) && final_sigma(points, i) {
            out.push(0x3c2);
        } else {
            map_one(point, effective, &mut out);
        }
        if point != u32::MAX && matches!(mode, Case::Title | Case::TitleSimple) {
            let property = properties(point);
            if property & 2 == 0 {
                in_word = property & 1 != 0;
            }
        }
    }
    out
}
fn final_sigma(points: &[u32], i: usize) -> bool {
    let previous = points[..i].iter().rev().find(|p| properties(**p) & 2 == 0);
    let next = points[i + 1..].iter().find(|p| properties(**p) & 2 == 0);
    previous.is_some_and(|p| properties(*p) & 1 != 0)
        && !next.is_some_and(|p| properties(*p) & 1 != 0)
}

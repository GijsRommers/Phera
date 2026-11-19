//! Mutations of fixture PHP, including `use A; $a = array(1);`, exercise every fix mode.

use super::{Enabled, Options, analyze, pipeline::converge};
use std::{fs, path::Path};

#[test]
fn mutated_sources_stay_in_bounds_and_fixes_converge() {
    let iterations = std::env::var("PHERA_LINT_MUTATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1024);
    let mut settings = Options::default();
    settings.fix = super::FixMode::All;
    settings.read_config(
        "[lint.groups]\nsuspicious = true\nunused = true\nmodernize = true\nstyle = true\nnaming = true\ncomplexity = true",
    ).expect("enable all lint groups");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/lint");
    let mut paths: Vec<_> = fs::read_dir(root)
        .expect("lint fixtures")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    paths.sort();
    let seeds: Vec<_> = paths
        .iter()
        .flat_map(|path| {
            ["bad.php", "good.php", "fixed.php"]
                .into_iter()
                .filter_map(|name| fs::read(path.join(name)).ok())
        })
        .collect();
    assert!(!seeds.is_empty());
    let enabled = Enabled::new(&settings);

    std::thread::Builder::new()
        .stack_size(phera_parser::PARSER_STACK_BYTES)
        .spawn(move || {
            let mut random = Bytes(0x0349_ec17_a572_d86b);
            let mut fixed_count = 0usize;
            for iteration in 0..iterations {
                let mut source = seeds[iteration % seeds.len()].clone();
                if iteration % 4 != 0 {
                    random.mutate(&mut source);
                }
                let name = "mutation.php";
                let (_, findings) = analyze(name, &source, &settings, &enabled, None);
                for edit in findings
                    .iter()
                    .filter_map(|finding| finding.fix.as_ref())
                    .flat_map(|fix| &fix.edits)
                {
                    assert!(
                        edit.span.start <= edit.span.end && edit.span.end <= source.len(),
                        "mutation {iteration} produced an out-of-range edit"
                    );
                }
                if let (Some(fixed), _) = converge(name, &source, &settings, &enabled, None) {
                    assert!(
                        phera_parser::parse_bytes_on_current_stack(&fixed).is_ok(),
                        "mutation {iteration} produced invalid PHP"
                    );
                    assert!(
                        converge(name, &fixed, &settings, &enabled, None)
                            .0
                            .is_none(),
                        "mutation {iteration} did not converge"
                    );
                    fixed_count += 1;
                }
            }
            println!("Processed {iterations} inputs and verified {fixed_count} edited candidates.");
        })
        .expect("start mutation test worker")
        .join()
        .expect("mutation test worker");
}

struct Bytes(u64);

impl Bytes {
    fn next(&mut self) -> usize {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) as usize
    }

    fn mutate(&mut self, source: &mut Vec<u8>) {
        const TOKENS: &[&[u8]] = &[
            b"array(",
            b")",
            b" /* note */ ",
            b"\\",
            b"\0",
            b"\r\n",
            b",",
            b"\xff",
        ];
        for _ in 0..self.next() % 4 + 1 {
            let offset = self.next() % (source.len() + 1);
            match self.next() % 4 {
                0 => {
                    source.splice(
                        offset..offset,
                        TOKENS[self.next() % TOKENS.len()].iter().copied(),
                    );
                }
                1 if offset < source.len() => {
                    source.remove(offset);
                }
                2 => {
                    source.truncate(offset);
                }
                _ if offset < source.len() => {
                    source[offset] = self.next() as u8;
                }
                _ => {}
            }
        }
    }
}

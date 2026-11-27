//! Phera AOT image format (`.phb`): deterministic encoding, integrity
//! verification, lazy safe decoding, and structural bytecode validation.
//!
//! All code in this crate is 100% safe Rust (`#![forbid(unsafe_code)]`).

pub mod decode;
pub mod encode;
pub mod error;
pub mod format;
pub mod opcodes;
pub mod strings;
pub mod validate;

pub use decode::AotImage;
pub use encode::{ImageInput, encode_image, write_encoded_image, write_image};
pub use error::ImageError;
pub use format::{
    BYTECODE_REVISION, FORMAT_VERSION, FileFlags, HEADER_LEN, ImageClassEntry, ImageFileEntry,
    ImageFlags, ImageHeader, ImageManifest, ImageTarget, MAGIC, SectionEntry, SectionKind,
    blake2b_64, blake2b_128, header_body_checksum, runtime_abi_hash,
};
pub use validate::validate_unit;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use phera_bytecode::facts::{
        Assumptions, BlastRadius, BuildMode, ClassBound, ClassFlags, ClassRef, DeclUniqueness,
        DynamicAudit, DynamicFinding, DynamicKind, FunctionBuildFacts, FunctionFlags, FunctionRef,
        LocalFlags, ProgramFacts, SoundMask, SoundType, UnitBuildFacts, UnitRef,
    };
    use phera_bytecode::{CompiledUnit, Instruction, Reg, Target};

    use super::*;

    fn sample_unit(source: &str, file: &str) -> Arc<CompiledUnit> {
        Arc::new(phera_compiler::compile(source, file).expect("sample php compiles"))
    }

    fn strip_metadata(unit: &CompiledUnit) -> CompiledUnit {
        let mut copy = unit.clone();
        copy.metadata = None;
        copy
    }

    #[test]
    fn round_trip_rich_php_unit_and_facts_deterministically() {
        let php = r#"<?php
declare(strict_types=1);

namespace App\Demo;

#[Attribute]
class Tag {
    public function __construct(public readonly string $name = 'default') {}
}

enum Status: string {
    case Active = 'active';
    case Disabled = 'disabled';
}

trait Loggable {
    public function log(string $msg): string {
        return "[log] " . $msg;
    }
}

/**
 * Sample service docblock.
 */
#[Tag('svc')]
final class Service {
    use Loggable {
        log as writeLog;
    }
    public const VERSION = 42;
    public int $count = 0;
    public string $label {
        get => "n=" . $this->count;
        set(string $v) { $this->count = strlen($v); }
    }

    public function compute(int $a, float $b = -0.0, int ...$rest): int|float {
        $sum = $a + $b;
        try {
            foreach ($rest as $i => $v) {
                $sum += $v + $i;
            }
        } catch (\Throwable $e) {
            return 0;
        } finally {
            $this->count++;
        }
        $fn = fn(int $x): int => $x * 2;
        return $fn((int)$sum);
    }
}

function gen(array $items): \Generator {
    foreach ($items as $k => $v) {
        yield $k => $v;
    }
    return count($items);
}
"#;
        let mut unit = (*sample_unit(php, "/app/src/Service.php")).clone();
        let fn_count = 1 + unit.functions.len();
        let cls_count = unit.classes.len();
        unit.facts = Some(Arc::new(UnitBuildFacts {
            unit: UnitRef(0),
            classes: (0..cls_count)
                .map(|idx| ClassRef(idx as u32))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            functions: std::iter::once(&unit.main)
                .chain(&unit.functions)
                .enumerate()
                .map(|(idx, f)| FunctionBuildFacts {
                    flags: if idx == 0 {
                        FunctionFlags::DYNAMIC_SCOPE
                    } else {
                        FunctionFlags::RETURN_PROVEN
                    },
                    locals: (0..f.locals.len())
                        .map(|li| {
                            if li == 0 {
                                LocalFlags::NONE
                            } else {
                                LocalFlags::MAYBE_REF
                            }
                        })
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                    block_starts: if f.instructions.is_empty() {
                        Box::new([])
                    } else {
                        vec![0].into_boxed_slice()
                    },
                    block_types: if f.instructions.is_empty() || f.registers == 0 {
                        Box::new([])
                    } else {
                        vec![(0, Reg(0), SoundType::INT)].into_boxed_slice()
                    },
                    calls: Box::new([]),
                    props: Box::new([]),
                    returns: SoundType {
                        mask: SoundMask::INT.union(SoundMask::FLOAT),
                        class: ClassBound::Any,
                    },
                })
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        }));
        let _ = fn_count;
        let unit = Arc::new(unit);

        let abi = runtime_abi_hash([7u8; 16]);
        let input = ImageInput {
            runtime_abi_hash: abi,
            manifest: ImageManifest {
                project_root: "/app".into(),
                mode: BuildMode::Strict,
                has_web: true,
                has_cli: false,
                allow: vec![DynamicKind::Reflection],
                framework: Some("symfony".into()),
                entries: vec!["public/index.php".into()],
                include_globs: vec!["src/**/*.php".into()],
                exclude_globs: vec!["tests/**".into()],
                include_dev: false,
            },
            files: vec![ImageFileEntry {
                relative_path: "src/Service.php".into(),
                canonical_path: "/app/src/Service.php".into(),
                unit_web: Some(0),
                unit_cli: None,
                source_len: php.len() as u64,
                source_hash: blake2b_128(php.as_bytes()),
                flags: FileFlags::ENTRY,
            }],
            classmap: vec![ImageClassEntry {
                name: "app\\demo\\service".into(),
                file_index: 0,
                class_id: 2,
                class_ref: Some(0),
            }],
            units: vec![unit.clone()],
            program_facts: Some(ProgramFacts {
                mode: BuildMode::Strict,
                units: vec!["src/Service.php".into()].into_boxed_slice(),
                classes: vec![phera_bytecode::facts::AotClassLayout {
                    name: "app\\demo\\service".into(),
                    decl: DeclUniqueness::Unique {
                        unit: UnitRef(0),
                        hoisted: true,
                    },
                    kind: phera_bytecode::ClassKind::Class,
                    parent: None,
                    ancestors: Box::new([]),
                    first_descendant: 0,
                    descendants: 0,
                    flags: ClassFlags::FINAL.union(ClassFlags::EFFECTIVELY_FINAL),
                    slots: Box::new([]),
                    methods: Box::new([]),
                }]
                .into_boxed_slice(),
                descendants: Box::new([]),
                functions: vec![(
                    "app\\demo\\gen".into(),
                    DeclUniqueness::Unique {
                        unit: UnitRef(0),
                        hoisted: true,
                    },
                    FunctionRef {
                        unit: UnitRef(0),
                        function: 1,
                    },
                )]
                .into_boxed_slice(),
                audit: DynamicAudit::default(),
            }),
            audit: DynamicAudit {
                findings: vec![DynamicFinding {
                    kind: DynamicKind::Reflection,
                    radius: BlastRadius::Class,
                    at: FunctionRef {
                        unit: UnitRef(0),
                        function: 0,
                    },
                    line: 12,
                    detail: "ReflectionClass".into(),
                }]
                .into_boxed_slice(),
                holds: Assumptions::SEALED_NAMES.union(Assumptions::SEALED_HIERARCHY),
            },
        };

        let bytes_a = encode_image(&input).expect("encode a");
        let bytes_b = encode_image(&input).expect("encode b");
        assert_eq!(bytes_a, bytes_b, ".phb encoding must be deterministic");

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("app.phb");
        write_image(&path, &input).expect("write image");

        let opened = AotImage::open(&path, Some(abi)).expect("open image");
        assert_eq!(opened.mode(), BuildMode::Strict);
        assert!(!opened.allows_eval());
        assert!(!opened.allows_class_alias());
        assert_eq!(opened.unit_count(), 1);
        assert_eq!(opened.manifest(), &input.manifest);
        assert_eq!(opened.audit(), &input.audit);
        assert_eq!(
            opened.program_facts().expect("program facts").audit,
            input.audit
        );
        // No cross-target fallback: Web-only image returns None for Cli target.
        assert!(
            opened
                .load_unit_for_path(
                    std::path::Path::new("/app/src/Service.php"),
                    ImageTarget::Cli
                )
                .expect("load cli")
                .is_none()
        );
        // Backslash traversal and bare relative paths never match canonical lookup.
        assert!(
            opened
                .find_exact_canonical(std::path::Path::new(
                    "/app/public/x\\..\\..\\src\\Service.php"
                ))
                .is_none()
        );
        assert!(
            opened
                .find_canonical(std::path::Path::new(
                    "/app/public/x\\..\\..\\src\\Service.php"
                ))
                .is_none()
        );
        assert!(
            opened
                .find_canonical(std::path::Path::new("src/Service.php"))
                .is_none()
        );
        assert!(opened.find_relative("src/Service.php").is_some());
        assert!(opened.find_relative("../src/Service.php").is_none());
        assert!(opened.find_relative("src\\Service.php").is_none());

        let decoded = opened
            .load_unit_for_path(
                std::path::Path::new("/app/src/Service.php"),
                ImageTarget::Web,
            )
            .expect("load unit")
            .expect("unit present");
        assert!(decoded.metadata.is_some());
        assert_eq!(strip_metadata(&decoded), strip_metadata(&unit));

        // Negative-zero float bit pattern preserved.
        let orig_neg_zero = unit.constants.iter().find_map(|c| match c {
            phera_bytecode::Constant::Float(f) if *f == 0.0 => Some(f.to_bits()),
            _ => None,
        });
        let dec_neg_zero = decoded.constants.iter().find_map(|c| match c {
            phera_bytecode::Constant::Float(f) if *f == 0.0 => Some(f.to_bits()),
            _ => None,
        });
        assert_eq!(orig_neg_zero, dec_neg_zero);
    }

    #[test]
    fn rejects_corrupted_bytes_abi_mismatch_and_invalid_bytecode() {
        let unit = sample_unit("<?php echo 1 + 2;", "/app/index.php");
        let abi = runtime_abi_hash([1u8; 16]);
        let input = ImageInput {
            runtime_abi_hash: abi,
            manifest: ImageManifest {
                project_root: "/app".into(),
                mode: BuildMode::Hybrid,
                has_web: true,
                has_cli: true,
                allow: Vec::new(),
                framework: None,
                entries: vec!["index.php".into()],
                include_globs: Vec::new(),
                exclude_globs: Vec::new(),
                include_dev: false,
            },
            files: vec![ImageFileEntry {
                relative_path: "index.php".into(),
                canonical_path: "/app/index.php".into(),
                unit_web: Some(0),
                unit_cli: Some(0),
                source_len: 17,
                source_hash: blake2b_128(b"<?php echo 1 + 2;"),
                flags: FileFlags::ENTRY,
            }],
            classmap: Vec::new(),
            units: vec![unit],
            program_facts: None,
            audit: DynamicAudit::default(),
        };

        let bytes = encode_image(&input).expect("encode");

        // ABI mismatch is rejected loudly.
        let wrong_abi = runtime_abi_hash([2u8; 16]);
        assert!(matches!(
            AotImage::from_bytes(bytes.clone().into(), Some(wrong_abi)),
            Err(ImageError::AbiMismatch { .. })
        ));

        // Flipping any body byte trips BLAKE2b checksum.
        let mut flipped = bytes.clone();
        let last = flipped.len() - 1;
        flipped[last] ^= 0xFF;
        assert!(matches!(
            AotImage::from_bytes(flipped.into(), Some(abi)),
            Err(ImageError::ChecksumMismatch)
        ));

        // Flipping a header flag bit (e.g. ALLOW_EVAL) also trips checksum!
        let mut header_tamper = bytes.clone();
        header_tamper[10] ^= ImageFlags::ALLOW_EVAL.0 as u8;
        assert!(matches!(
            AotImage::from_bytes(header_tamper.into(), Some(abi)),
            Err(ImageError::ChecksumMismatch)
        ));

        // Unknown header flag bit is rejected even if checksum is recomputed.
        let mut unknown_flag = bytes.clone();
        let bad_flags = u16::from_le_bytes([unknown_flag[10], unknown_flag[11]]) | (1 << 15);
        unknown_flag[10..12].copy_from_slice(&bad_flags.to_le_bytes());
        let recomputed = header_body_checksum(
            &unknown_flag[0..32],
            &unknown_flag[48..HEADER_LEN as usize],
            &unknown_flag[HEADER_LEN as usize..],
        );
        unknown_flag[32..48].copy_from_slice(&recomputed);
        assert!(matches!(
            AotImage::from_bytes(unknown_flag.into(), Some(abi)),
            Err(ImageError::Corrupt(_))
        ));

        // Non-canonical instruction padding byte is rejected.
        let mut non_canon = opcodes::encode_instruction(Instruction::LoadNull { dst: Reg(0) });
        non_canon[15] = 1;
        assert!(matches!(
            opcodes::decode_instruction(&non_canon),
            Err(ImageError::Corrupt(_))
        ));

        // Corrupt magic rejected.
        let mut bad_magic = bytes;
        bad_magic[0] = b'X';
        assert!(matches!(
            AotImage::from_bytes(bad_magic.into(), Some(abi)),
            Err(ImageError::InvalidMagic)
        ));

        // Validator rejects jump target == ins_len (must be strictly < ins_len).
        let mut invalid = CompiledUnit::default();
        invalid.main.registers = 1;
        invalid.main.instructions = vec![Instruction::Jump { target: Target(1) }].into();
        invalid.main.lines = vec![1];
        assert!(matches!(
            validate_unit(&invalid),
            Err(ImageError::Validation(_))
        ));

        invalid.main.instructions = vec![Instruction::LoadNull { dst: Reg(5) }].into();
        assert!(matches!(
            validate_unit(&invalid),
            Err(ImageError::Validation(_))
        ));
    }

    #[test]
    fn golden_opcode_schema_fingerprint_catches_accidental_drift() {
        // Explicit opcode schema digest for the current 124-opcode ISA v1 table.
        // Update intentionally ONLY when adding new permanent opcode entries.
        let digest = blake2b_128(opcodes::schema_description().as_bytes());
        assert_eq!(
            opcodes::encode_instruction(Instruction::Nop),
            [0u8; opcodes::ENCODED_INSTRUCTION_LEN]
        );
        assert_eq!(
            opcodes::decode_instruction(&[0u8; opcodes::ENCODED_INSTRUCTION_LEN]).unwrap(),
            Instruction::Nop
        );
        // Round-trip sanity check on high-numbered opcode.
        let inst = Instruction::CallNativeDirect {
            function: phera_bytecode::SymbolId(7),
            args: Reg(2),
            count: 3,
            dst: Reg(1),
            result: phera_bytecode::CallResult::Used,
        };
        let encoded = opcodes::encode_instruction(inst);
        assert_eq!(opcodes::decode_instruction(&encoded).unwrap(), inst);
        const EXPECTED_SCHEMA_DIGEST: [u8; 16] = [
            0x62, 0x5a, 0x98, 0xee, 0x71, 0xd8, 0x55, 0x58, 0xaf, 0x24, 0x9f, 0xbe, 0x66, 0xca,
            0xad, 0x37,
        ];
        assert_eq!(digest, EXPECTED_SCHEMA_DIGEST);
    }
}

//! Safe, lazy `.phb` binary decoder and opened [`AotImage`] reader.
//!
//! Index sections (`MANIFEST`, `STRINGS`, `FILES`, `CLASSMAP`, `FACTS`, `AUDIT`,
//! and the `UNITS` directory) are verified and decoded eagerly on open. Unit
//! bytecode blobs are read on demand with safe positional file reads
//! (`FileExt::read_exact_at` on Unix, synchronized `Seek + Read` on non-Unix)
//! and cached in per-unit [`OnceLock`]s.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock};

use phera_bytecode::facts::{
    AotClassLayout, AotMethod, AotPropSlot, Assumptions, BlastRadius, BuildMode, CallFlags,
    ClassBound, ClassFlags, ClassRef, DeclUniqueness, DynamicAudit, DynamicFinding, DynamicKind,
    FunctionBuildFacts, FunctionFlags, FunctionRef, LocalFlags, MethodFlags, ProgramFacts,
    PropAccess, PropFlags, ResolvedCall, ResolvedProp, SoundMask, SoundType, UnitBuildFacts,
    UnitRef,
};
use phera_bytecode::{
    ArgumentLayout, ClassConstant, ClassDeclaration, ClassId, ClassKind, CompiledUnit, ConstId,
    Constant, ConstantKey, DeclarationWarning, EnumBacking, EnumCase, Function, FunctionId,
    Handler, Initializer, MethodDeclaration, MethodSite, NameId, Parameter, Path as BcPath,
    PathBase, PropertyDeclaration, PropertyHook, PropertyHooks, Reg, SiteArgument, Step, Symbol,
    SymbolId, Target, TraitAdaptation, TraitUse, Type, TypeId, TypeMask, UnitMetadata, Visibility,
    reflection,
};
use phera_source::{CompileDiagnostic, CompileDiagnosticKind, LineTable, Span};

use crate::error::ImageError;
use crate::format::{
    FORMAT_VERSION, FileFlags, HEADER_LEN, ImageClassEntry, ImageFileEntry, ImageFlags,
    ImageHeader, ImageManifest, ImageTarget, MAGIC, SECTION_ENTRY_LEN, SectionEntry, SectionKind,
    UNIT_ENTRY_LEN, UnitDirEntry, blake2b_64, decode_allow_mask, header_body_checksum,
};
use crate::opcodes::{ENCODED_INSTRUCTION_LEN, decode_instruction};
use crate::strings::{StringId, StringPool};
use crate::validate::validate_unit;

/// Backing byte source for lazy unit reads (100% safe Rust, zero `mmap`).
enum Backing {
    #[cfg(unix)]
    File(File),
    #[cfg(not(unix))]
    File(std::sync::Mutex<File>),
    Bytes(Arc<[u8]>),
}

impl Backing {
    fn read_exact_at(&self, offset: u64, len: usize) -> Result<Vec<u8>, ImageError> {
        let mut buf = vec![0u8; len];
        match self {
            #[cfg(unix)]
            Self::File(file) => {
                use std::os::unix::fs::FileExt;
                file.read_exact_at(&mut buf, offset)?;
            }
            #[cfg(not(unix))]
            Self::File(file) => {
                use std::io::Seek;
                let mut guard = file
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                guard.seek(std::io::SeekFrom::Start(offset))?;
                guard.read_exact(&mut buf)?;
            }
            Self::Bytes(bytes) => {
                let start = usize::try_from(offset)
                    .map_err(|_| ImageError::Corrupt("offset exceeds usize".into()))?;
                let end = start
                    .checked_add(len)
                    .ok_or_else(|| ImageError::Corrupt("range overflow".into()))?;
                let slice = bytes
                    .get(start..end)
                    .ok_or_else(|| ImageError::Corrupt("slice out of bounds".into()))?;
                buf.copy_from_slice(slice);
            }
        }
        Ok(buf)
    }
}

/// An opened, integrity-verified `.phb` production image shared across workers.
pub struct AotImage {
    header: ImageHeader,
    manifest: ImageManifest,
    strings: StringPool,
    files: Box<[ImageFileEntry]>,
    by_canonical: HashMap<String, usize>,
    by_relative: HashMap<String, usize>,
    classmap: Box<[ImageClassEntry]>,
    units_section_offset: u64,
    unit_dir: Box<[UnitDirEntry]>,
    unit_cache: Box<[OnceLock<Arc<CompiledUnit>>]>,
    program_facts: Option<Arc<ProgramFacts>>,
    audit: DynamicAudit,
    backing: Backing,
}

impl AotImage {
    /// Opens and verifies a `.phb` image from disk against `expected_abi_hash`
    /// (pass `None` to skip the ABI check, e.g. for inspection tools).
    pub fn open(path: &Path, expected_abi_hash: Option<[u8; 16]>) -> Result<Arc<Self>, ImageError> {
        let mut file = File::open(path)?;
        let mut all = Vec::new();
        file.read_to_end(&mut all)?;
        #[cfg(unix)]
        let backing = Backing::File(file);
        #[cfg(not(unix))]
        let backing = Backing::File(std::sync::Mutex::new(file));
        Self::from_verified_bytes(&all, backing, expected_abi_hash)
    }

    /// Opens and verifies a `.phb` image directly from an in-memory buffer.
    pub fn from_bytes(
        bytes: Arc<[u8]>,
        expected_abi_hash: Option<[u8; 16]>,
    ) -> Result<Arc<Self>, ImageError> {
        let backing = Backing::Bytes(bytes.clone());
        Self::from_verified_bytes(&bytes, backing, expected_abi_hash)
    }

    fn from_verified_bytes(
        all: &[u8],
        backing: Backing,
        expected_abi_hash: Option<[u8; 16]>,
    ) -> Result<Arc<Self>, ImageError> {
        let header = decode_header(all)?;
        if header.file_len != all.len() as u64 {
            return Err(ImageError::ChecksumMismatch);
        }
        if let Some(expected) = expected_abi_hash
            && header.runtime_abi_hash != expected
        {
            return Err(ImageError::AbiMismatch {
                found: header.runtime_abi_hash,
                expected,
            });
        }
        let body = &all[HEADER_LEN as usize..];
        if header_body_checksum(&all[0..32], &all[48..HEADER_LEN as usize], body)
            != header.body_checksum
        {
            return Err(ImageError::ChecksumMismatch);
        }

        let sections = decode_directory(all, &header)?;
        let section_slice = |kind: SectionKind| -> Result<Option<(u64, &[u8])>, ImageError> {
            let Some(entry) = sections.iter().find(|s| s.kind == kind) else {
                return Ok(None);
            };
            let start = usize::try_from(entry.offset)
                .map_err(|_| ImageError::Corrupt("section offset overflow".into()))?;
            let len = usize::try_from(entry.len)
                .map_err(|_| ImageError::Corrupt("section len overflow".into()))?;
            let end = start
                .checked_add(len)
                .ok_or_else(|| ImageError::Corrupt("section range overflow".into()))?;
            let slice = all
                .get(start..end)
                .ok_or_else(|| ImageError::Corrupt("section out of file bounds".into()))?;
            if blake2b_64(slice) != entry.check {
                return Err(ImageError::SectionChecksumMismatch {
                    section: kind.name(),
                });
            }
            Ok(Some((entry.offset, slice)))
        };

        let (_, strings_raw) = section_slice(SectionKind::Strings)?
            .ok_or_else(|| ImageError::Corrupt("missing STRINGS section".into()))?;
        let strings = StringPool::decode(strings_raw)?;

        let (_, manifest_raw) = section_slice(SectionKind::Manifest)?
            .ok_or_else(|| ImageError::Corrupt("missing MANIFEST section".into()))?;
        let manifest = decode_manifest(manifest_raw, &strings)?;

        let (_, files_raw) = section_slice(SectionKind::Files)?
            .ok_or_else(|| ImageError::Corrupt("missing FILES section".into()))?;
        let files = decode_files(files_raw, &strings)?;

        let (_, classmap_raw) = section_slice(SectionKind::Classmap)?
            .ok_or_else(|| ImageError::Corrupt("missing CLASSMAP section".into()))?;
        let classmap = decode_classmap(classmap_raw, &strings, files.len())?;

        let (units_section_offset, units_raw) = section_slice(SectionKind::Units)?
            .ok_or_else(|| ImageError::Corrupt("missing UNITS section".into()))?;
        let unit_dir = decode_unit_directory(units_raw)?;

        for entry in &files {
            if let Some(id) = entry.unit_web
                && (id as usize) >= unit_dir.len()
            {
                return Err(ImageError::Corrupt(format!(
                    "file `{}` references invalid web unit #{id}",
                    entry.relative_path
                )));
            }
            if let Some(id) = entry.unit_cli
                && (id as usize) >= unit_dir.len()
            {
                return Err(ImageError::Corrupt(format!(
                    "file `{}` references invalid cli unit #{id}",
                    entry.relative_path
                )));
            }
        }

        let audit = match section_slice(SectionKind::Audit)? {
            Some((_, raw)) => decode_audit(raw, &strings)?,
            None => DynamicAudit::default(),
        };
        let program_facts = match section_slice(SectionKind::Facts)? {
            Some((_, raw)) => {
                let mut facts = decode_program_facts(raw, &strings)?;
                facts.audit = audit.clone();
                Some(Arc::new(facts))
            }
            None => None,
        };

        let expected_flags = expected_header_flags(&manifest, program_facts.is_some());
        if header.flags != expected_flags {
            return Err(ImageError::Corrupt(
                "header flags disagree with MANIFEST/FACTS sections".into(),
            ));
        }

        let mut by_canonical = HashMap::with_capacity(files.len());
        let mut by_relative = HashMap::with_capacity(files.len());
        for (idx, entry) in files.iter().enumerate() {
            by_canonical.insert(entry.canonical_path.clone(), idx);
            by_relative.insert(entry.relative_path.clone(), idx);
        }

        let unit_cache = (0..unit_dir.len())
            .map(|_| OnceLock::new())
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Ok(Arc::new(Self {
            header,
            manifest,
            strings,
            files: files.into_boxed_slice(),
            by_canonical,
            by_relative,
            classmap: classmap.into_boxed_slice(),
            units_section_offset,
            unit_dir: unit_dir.into_boxed_slice(),
            unit_cache,
            program_facts,
            audit,
            backing,
        }))
    }

    pub fn header(&self) -> &ImageHeader {
        &self.header
    }

    pub fn manifest(&self) -> &ImageManifest {
        &self.manifest
    }

    pub fn mode(&self) -> BuildMode {
        self.manifest.mode
    }

    pub fn allows_eval(&self) -> bool {
        self.mode() == BuildMode::Hybrid || self.manifest.allow.contains(&DynamicKind::Eval)
    }

    pub fn allows_class_alias(&self) -> bool {
        self.mode() == BuildMode::Hybrid || self.manifest.allow.contains(&DynamicKind::ClassAlias)
    }

    pub fn files(&self) -> &[ImageFileEntry] {
        &self.files
    }

    pub fn classmap(&self) -> &[ImageClassEntry] {
        &self.classmap
    }

    pub fn unit_count(&self) -> usize {
        self.unit_dir.len()
    }

    pub fn program_facts(&self) -> Option<&Arc<ProgramFacts>> {
        self.program_facts.as_ref()
    }

    pub fn audit(&self) -> &DynamicAudit {
        &self.audit
    }

    /// Checks that this image was built for `expected_root` (canonicalized when
    /// possible). Because `__FILE__` and `__DIR__` are baked into bytecode, an
    /// image is bound to its project root.
    pub fn verify_project_root(&self, expected_root: &Path) -> Result<(), ImageError> {
        let canonical = expected_root
            .canonicalize()
            .unwrap_or_else(|_| expected_root.to_path_buf());
        let expected_str = canonical.to_string_lossy();
        if self.manifest.project_root != expected_str {
            return Err(ImageError::RootMismatch {
                image_root: self.manifest.project_root.clone(),
                expected_root: expected_str.into_owned(),
            });
        }
        Ok(())
    }

    /// Verifies that a `phera serve` document root and detected project root
    /// belong to the project this image was built from (allowing document roots
    /// like `public/` inside the project root even without `composer.json` or
    /// `phera.toml`, while rejecting a distinct nested project with its own
    /// project root marker).
    pub fn verify_serve_root(
        &self,
        document_root: &Path,
        project_root: &Path,
    ) -> Result<(), ImageError> {
        let image_root = Path::new(&self.manifest.project_root);
        let canonical_proj = canonical_or_clean(project_root);
        if canonical_proj == image_root {
            return Ok(());
        }
        let canonical_doc = canonical_or_clean(document_root);
        let is_distinct_nested_project = canonical_doc != image_root
            && ["phera.toml", "composer.json", ".git"]
                .iter()
                .any(|marker| canonical_doc.join(marker).exists());
        if canonical_proj == canonical_doc
            && canonical_doc.starts_with(image_root)
            && !is_distinct_nested_project
        {
            return Ok(());
        }
        Err(ImageError::RootMismatch {
            image_root: self.manifest.project_root.clone(),
            expected_root: canonical_proj.to_string_lossy().into_owned(),
        })
    }

    /// Exact lookup by canonical path with no lexical normalization or relative
    /// fallback. Safe for HTTP document-root script resolution.
    pub fn find_exact_canonical(&self, path: &Path) -> Option<&ImageFileEntry> {
        let key = path.to_string_lossy();
        self.by_canonical
            .get(key.as_ref())
            .and_then(|&idx| self.files.get(idx))
    }

    /// Looks up a file entry by an absolute path (exact match first, then
    /// cleaned using native OS path components only — never rewriting `\` on
    /// Unix and never matching bare relative paths against `project_root`).
    pub fn find_canonical(&self, path: &Path) -> Option<&ImageFileEntry> {
        if !path.is_absolute() {
            return None;
        }
        if let Some(entry) = self.find_exact_canonical(path) {
            return Some(entry);
        }
        let cleaned = clean_native_path(path);
        let cleaned_str = cleaned.to_string_lossy();
        self.by_canonical
            .get(cleaned_str.as_ref())
            .and_then(|&idx| self.files.get(idx))
    }

    /// Explicit lookup by project-relative path (using `/` separators, with no
    /// leading `/`, `..`, or `\`). Used only for manifest entries and explicit
    /// CLI image script names when no local file exists in the working directory.
    pub fn find_relative(&self, relative_path: &str) -> Option<&ImageFileEntry> {
        if relative_path.is_empty()
            || relative_path.starts_with('/')
            || relative_path.contains('\\')
            || relative_path.contains('\0')
        {
            return None;
        }
        let trimmed = relative_path.strip_prefix("./").unwrap_or(relative_path);
        if trimmed
            .split('/')
            .any(|seg| seg.is_empty() || seg == "." || seg == "..")
        {
            return None;
        }
        self.by_relative
            .get(trimmed)
            .and_then(|&idx| self.files.get(idx))
    }

    /// Decodes (lazily, caching in `OnceLock`) the [`CompiledUnit`] for `path`
    /// and `target`. Never crosses between Web and CLI targets.
    pub fn load_unit_for_path(
        &self,
        path: &Path,
        target: ImageTarget,
    ) -> Result<Option<Arc<CompiledUnit>>, ImageError> {
        let Some(entry) = self.find_canonical(path) else {
            return Ok(None);
        };
        let unit_id = match target {
            ImageTarget::Web => entry.unit_web,
            ImageTarget::Cli => entry.unit_cli,
        };
        match unit_id {
            Some(id) => self.load_unit(id).map(Some),
            None => Ok(None),
        }
    }

    /// Lazily reads, checksums, decodes, validates, and caches unit `unit_id`.
    pub fn load_unit(&self, unit_id: u32) -> Result<Arc<CompiledUnit>, ImageError> {
        let idx = unit_id as usize;
        let slot = self
            .unit_cache
            .get(idx)
            .ok_or_else(|| ImageError::Corrupt(format!("unit id {unit_id} out of bounds")))?;
        if let Some(unit) = slot.get() {
            return Ok(unit.clone());
        }
        let dir = self.unit_dir[idx];
        let abs_offset = self
            .units_section_offset
            .checked_add(dir.rel_offset)
            .ok_or_else(|| ImageError::Corrupt("unit absolute offset overflow".into()))?;
        let raw = self.backing.read_exact_at(abs_offset, dir.len as usize)?;
        if blake2b_64(&raw) != dir.check {
            return Err(ImageError::UnitChecksumMismatch { unit: unit_id });
        }
        let mut unit = decode_unit(&raw, &self.strings)?;
        validate_unit(&unit)?;
        unit.metadata = Some(Arc::new(UnitMetadata::new(&unit)));
        let arc = Arc::new(unit);
        let _ = slot.set(arc.clone());
        Ok(slot.get().cloned().unwrap_or(arc))
    }

    /// Eagerly decodes and validates every unit in the image (used by `--verify`).
    pub fn verify_all_units(&self) -> Result<usize, ImageError> {
        for idx in 0..self.unit_dir.len() {
            let _ = self.load_unit(idx as u32)?;
        }
        Ok(self.unit_dir.len())
    }
}

fn expected_header_flags(manifest: &ImageManifest, has_facts: bool) -> ImageFlags {
    let mut flags = ImageFlags::NONE;
    if manifest.mode == BuildMode::Strict {
        flags = flags.union(ImageFlags::STRICT);
    }
    if has_facts {
        flags = flags.union(ImageFlags::HAS_FACTS);
    }
    if manifest.has_web {
        flags = flags.union(ImageFlags::WEB_UNITS);
    }
    if manifest.has_cli {
        flags = flags.union(ImageFlags::CLI_UNITS);
    }
    if manifest.allow.contains(&DynamicKind::Eval) {
        flags = flags.union(ImageFlags::ALLOW_EVAL);
    }
    if manifest.allow.contains(&DynamicKind::ClassAlias) {
        flags = flags.union(ImageFlags::ALLOW_CLASS_ALIAS);
    }
    flags
}

fn canonical_or_clean(path: &Path) -> PathBuf {
    path.canonicalize()
        .unwrap_or_else(|_| clean_native_path(path))
}

/// Cleans `.` and `..` using Rust's native OS [`Path::components`] ONLY.
/// Never treats `\` as a directory separator on Unix.
fn clean_native_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    let mut depth = 0usize;
    let is_abs = path.is_absolute();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => out.push(prefix.as_os_str()),
            Component::RootDir => out.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if depth > 0 {
                    out.pop();
                    depth -= 1;
                } else if !is_abs {
                    out.push(component.as_os_str());
                }
            }
            Component::Normal(part) => {
                out.push(part);
                depth += 1;
            }
        }
    }
    out
}

/// Upper bound on total string/byte bytes copied out of the shared string pool
/// during a single section or unit decode, preventing string-pool amplification
/// bombs on untrusted images.
const MAX_DECODED_BYTES: usize = 128 * 1024 * 1024;

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
    decoded_bytes: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            pos: 0,
            decoded_bytes: 0,
        }
    }

    fn account_bytes(&mut self, len: usize) -> Result<(), ImageError> {
        let next = self
            .decoded_bytes
            .checked_add(len)
            .ok_or_else(|| ImageError::Corrupt("decoded byte budget overflow".into()))?;
        if next > MAX_DECODED_BYTES {
            return Err(ImageError::Corrupt(format!(
                "decoded byte budget exceeded ({MAX_DECODED_BYTES} bytes)"
            )));
        }
        self.decoded_bytes = next;
        Ok(())
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], ImageError> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or_else(|| ImageError::Corrupt("cursor overflow".into()))?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or_else(|| ImageError::Corrupt("unexpected end of section/unit payload".into()))?;
        self.pos = end;
        Ok(slice)
    }

    fn finish(&self, context: &str) -> Result<(), ImageError> {
        if self.pos != self.bytes.len() {
            return Err(ImageError::Corrupt(format!(
                "{context}: {} trailing bytes",
                self.bytes.len() - self.pos
            )));
        }
        Ok(())
    }

    fn u8(&mut self) -> Result<u8, ImageError> {
        Ok(self.take(1)?[0])
    }

    fn bool(&mut self, field: &str) -> Result<bool, ImageError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(ImageError::Corrupt(format!(
                "invalid bool {other} for {field}"
            ))),
        }
    }

    fn u16(&mut self) -> Result<u16, ImageError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, ImageError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64, ImageError> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    fn i64(&mut self) -> Result<i64, ImageError> {
        Ok(self.u64()? as i64)
    }

    fn bytes16(&mut self) -> Result<[u8; 16], ImageError> {
        let b = self.take(16)?;
        let mut out = [0u8; 16];
        out.copy_from_slice(b);
        Ok(out)
    }

    fn len(&mut self) -> Result<usize, ImageError> {
        Ok(self.u32()? as usize)
    }

    fn str(&mut self, pool: &StringPool) -> Result<String, ImageError> {
        let id = StringId(self.u32()?);
        let value = pool.get_str(id)?;
        self.account_bytes(value.len())?;
        Ok(value.to_owned())
    }

    fn opt_str(&mut self, pool: &StringPool) -> Result<Option<String>, ImageError> {
        let raw = self.u32()?;
        if raw == u32::MAX {
            Ok(None)
        } else {
            let value = pool.get_str(StringId(raw))?;
            self.account_bytes(value.len())?;
            Ok(Some(value.to_owned()))
        }
    }

    fn pool_bytes(&mut self, pool: &StringPool) -> Result<Vec<u8>, ImageError> {
        let id = StringId(self.u32()?);
        let value = pool.get_bytes(id)?;
        self.account_bytes(value.len())?;
        Ok(value.to_vec())
    }

    fn str_vec(&mut self, pool: &StringPool) -> Result<Vec<String>, ImageError> {
        let count = self.len()?;
        let mut out = Vec::with_capacity(count.min(4096));
        for _ in 0..count {
            out.push(self.str(pool)?);
        }
        Ok(out)
    }
}

fn decode_header(bytes: &[u8]) -> Result<ImageHeader, ImageError> {
    if bytes.len() < HEADER_LEN as usize {
        return Err(ImageError::InvalidMagic);
    }
    if bytes[0..8] != MAGIC {
        return Err(ImageError::InvalidMagic);
    }
    let mut cur = Cursor::new(&bytes[8..HEADER_LEN as usize]);
    let format_version = cur.u16()?;
    if format_version != FORMAT_VERSION {
        return Err(ImageError::UnsupportedVersion {
            found: format_version,
            expected: FORMAT_VERSION,
        });
    }
    let flags = ImageFlags(cur.u16()?);
    if flags.0 & !ImageFlags::KNOWN_MASK.0 != 0 {
        return Err(ImageError::Corrupt(format!(
            "unknown header flag bits: {:#06x}",
            flags.0 & !ImageFlags::KNOWN_MASK.0
        )));
    }
    let header_len = cur.u32()?;
    if header_len != HEADER_LEN {
        return Err(ImageError::Corrupt(format!(
            "unexpected header_len {header_len}"
        )));
    }
    let runtime_abi_hash = cur.bytes16()?;
    let body_checksum = cur.bytes16()?;
    let file_len = cur.u64()?;
    let section_count = cur.u32()?;
    let reserved = cur.u32()?;
    if reserved != 0 {
        return Err(ImageError::Corrupt("non-zero reserved header field".into()));
    }
    Ok(ImageHeader {
        format_version,
        flags,
        header_len,
        runtime_abi_hash,
        body_checksum,
        file_len,
        section_count,
    })
}

fn decode_directory(bytes: &[u8], header: &ImageHeader) -> Result<Vec<SectionEntry>, ImageError> {
    let count = header.section_count as usize;
    let dir_bytes = count
        .checked_mul(SECTION_ENTRY_LEN)
        .ok_or_else(|| ImageError::Corrupt("section count overflow".into()))?;
    let start = HEADER_LEN as usize;
    let end = start
        .checked_add(dir_bytes)
        .ok_or_else(|| ImageError::Corrupt("directory end overflow".into()))?;
    let slice = bytes
        .get(start..end)
        .ok_or_else(|| ImageError::Corrupt("section directory truncated".into()))?;
    let mut cur = Cursor::new(slice);
    let mut entries = Vec::with_capacity(count.min(32));
    for _ in 0..count {
        let kind_raw = cur.u32()?;
        let kind = SectionKind::from_u32(kind_raw)
            .ok_or_else(|| ImageError::Corrupt(format!("unknown section kind {kind_raw}")))?;
        let flags = cur.u32()?;
        let offset = cur.u64()?;
        let len = cur.u64()?;
        let check = cur.u64()?;
        entries.push(SectionEntry {
            kind,
            flags,
            offset,
            len,
            check,
        });
    }
    Ok(entries)
}

fn decode_manifest(raw: &[u8], pool: &StringPool) -> Result<ImageManifest, ImageError> {
    let mut cur = Cursor::new(raw);
    let project_root = cur.str(pool)?;
    let mode = decode_build_mode(cur.u8()?)?;
    let target_bits = cur.u8()?;
    let include_dev = cur.bool("manifest.include_dev")?;
    let _reserved = cur.u8()?;
    let allow_mask = cur.u32()?;
    let framework = cur.opt_str(pool)?;
    let entries = cur.str_vec(pool)?;
    let include_globs = cur.str_vec(pool)?;
    let exclude_globs = cur.str_vec(pool)?;
    cur.finish("MANIFEST")?;
    Ok(ImageManifest {
        project_root,
        mode,
        has_web: (target_bits & 1) != 0,
        has_cli: (target_bits & 2) != 0,
        allow: decode_allow_mask(allow_mask),
        framework,
        entries,
        include_globs,
        exclude_globs,
        include_dev,
    })
}

fn decode_files(raw: &[u8], pool: &StringPool) -> Result<Vec<ImageFileEntry>, ImageError> {
    let mut cur = Cursor::new(raw);
    let count = cur.len()?;
    let mut out = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let relative_path = cur.str(pool)?;
        let canonical_path = cur.str(pool)?;
        let web_raw = cur.u32()?;
        let cli_raw = cur.u32()?;
        let source_len = cur.u64()?;
        let source_hash = cur.bytes16()?;
        let flags = FileFlags(cur.u32()?);
        out.push(ImageFileEntry {
            relative_path,
            canonical_path,
            unit_web: (web_raw != u32::MAX).then_some(web_raw),
            unit_cli: (cli_raw != u32::MAX).then_some(cli_raw),
            source_len,
            source_hash,
            flags,
        });
    }
    cur.finish("FILES")?;
    Ok(out)
}

fn decode_classmap(
    raw: &[u8],
    pool: &StringPool,
    file_count: usize,
) -> Result<Vec<ImageClassEntry>, ImageError> {
    let mut cur = Cursor::new(raw);
    let count = cur.len()?;
    let mut out = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let name = cur.str(pool)?;
        let file_index = cur.u32()?;
        if (file_index as usize) >= file_count {
            return Err(ImageError::Corrupt(format!(
                "classmap entry `{name}` references invalid file_index {file_index}"
            )));
        }
        let class_id = cur.u32()?;
        let class_ref_raw = cur.u32()?;
        out.push(ImageClassEntry {
            name,
            file_index,
            class_id,
            class_ref: (class_ref_raw != u32::MAX).then_some(class_ref_raw),
        });
    }
    cur.finish("CLASSMAP")?;
    Ok(out)
}

fn decode_unit_directory(raw: &[u8]) -> Result<Vec<UnitDirEntry>, ImageError> {
    let mut cur = Cursor::new(raw);
    let count = cur.len()?;
    let min_dir_end = 4u64
        .checked_add(
            (count as u64)
                .checked_mul(UNIT_ENTRY_LEN as u64)
                .ok_or_else(|| ImageError::Corrupt("unit directory len overflow".into()))?,
        )
        .ok_or_else(|| ImageError::Corrupt("unit directory end overflow".into()))?;
    let section_len = raw.len() as u64;
    if min_dir_end > section_len {
        return Err(ImageError::Corrupt("UNITS directory truncated".into()));
    }
    let mut out = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let rel_offset = cur.u64()?;
        let len = cur.u32()?;
        let reserved = cur.u32()?;
        let check = cur.u64()?;
        if reserved != 0 {
            return Err(ImageError::Corrupt(
                "non-zero reserved unit directory field".into(),
            ));
        }
        let end = rel_offset
            .checked_add(u64::from(len))
            .ok_or_else(|| ImageError::Corrupt("unit blob range overflow".into()))?;
        if rel_offset < min_dir_end || end > section_len {
            return Err(ImageError::Corrupt(
                "unit blob range outside UNITS section".into(),
            ));
        }
        out.push(UnitDirEntry {
            rel_offset,
            len,
            check,
        });
    }
    Ok(out)
}

fn decode_unit(raw: &[u8], pool: &StringPool) -> Result<CompiledUnit, ImageError> {
    let mut cur = Cursor::new(raw);
    let file = cur.str(pool)?;
    let has_link_attributes = cur.bool("unit.has_link_attributes")?;

    let const_meta_len = cur.len()?;
    let mut constant_metadata = HashMap::with_capacity(const_meta_len.min(256));
    for _ in 0..const_meta_len {
        let name = cur.str(pool)?;
        let meta = decode_decl_meta(&mut cur, pool)?;
        constant_metadata.insert(name, meta);
    }

    let warn_len = cur.len()?;
    let mut declaration_warnings = Vec::with_capacity(warn_len.min(256));
    for _ in 0..warn_len {
        let message = cur.str(pool)?;
        let line = cur.u32()?;
        declaration_warnings.push(DeclarationWarning { message, line });
    }

    let diag_len = cur.len()?;
    let mut scanner_diagnostics = Vec::with_capacity(diag_len.min(256));
    for _ in 0..diag_len {
        let kind = match cur.u8()? {
            0 => CompileDiagnosticKind::Warning,
            1 => CompileDiagnosticKind::Deprecated,
            other => {
                return Err(ImageError::Corrupt(format!(
                    "invalid CompileDiagnosticKind {other}"
                )));
            }
        };
        let message = cur.str(pool)?;
        let line = cur.u32()? as usize;
        scanner_diagnostics.push(CompileDiagnostic {
            kind,
            message,
            line,
        });
    }

    let names = cur.str_vec(pool)?;

    let sym_len = cur.len()?;
    let mut symbols = Vec::with_capacity(sym_len.min(1024));
    for _ in 0..sym_len {
        let name = cur.str(pool)?;
        let fallback = cur.opt_str(pool)?;
        let written = cur.str(pool)?;
        symbols.push(Symbol {
            name,
            fallback,
            written,
        });
    }

    let type_len = cur.len()?;
    let mut types = Vec::with_capacity(type_len.min(512));
    for _ in 0..type_len {
        let mask = TypeMask(cur.u16()?);
        let intersection = cur.bool("type.intersection")?;
        let display = cur.str(pool)?;
        let cls_len = cur.len()?;
        let mut classes = Vec::with_capacity(cls_len.min(64));
        for _ in 0..cls_len {
            classes.push(NameId(cur.u32()?));
        }
        types.push(Type {
            mask,
            classes,
            intersection,
            display,
        });
    }

    let const_len = cur.len()?;
    let mut constants = Vec::with_capacity(const_len.min(1024));
    for _ in 0..const_len {
        constants.push(decode_constant(&mut cur, pool, 0)?);
    }

    let main = decode_function(&mut cur, pool)?;
    let fn_len = cur.len()?;
    let mut functions = Vec::with_capacity(fn_len.min(1024));
    for _ in 0..fn_len {
        functions.push(decode_function(&mut cur, pool)?);
    }

    let hoisted_len = cur.len()?;
    let mut hoisted = Vec::with_capacity(hoisted_len.min(512));
    for _ in 0..hoisted_len {
        hoisted.push(FunctionId(cur.u32()?));
    }

    let hoisted_cls_len = cur.len()?;
    let mut hoisted_classes = Vec::with_capacity(hoisted_cls_len.min(512));
    for _ in 0..hoisted_cls_len {
        hoisted_classes.push(ClassId(cur.u32()?));
    }

    let cls_len = cur.len()?;
    let mut classes = Vec::with_capacity(cls_len.min(512));
    for _ in 0..cls_len {
        classes.push(decode_class(&mut cur, pool)?);
    }

    let fn_meta_len = cur.len()?;
    let mut function_metadata = Vec::with_capacity(fn_meta_len.min(512));
    for _ in 0..fn_meta_len {
        let declaration = decode_decl_meta(&mut cur, pool)?;
        let param_len = cur.len()?;
        let mut parameters = Vec::with_capacity(param_len.min(64));
        for _ in 0..param_len {
            let attr_len = cur.len()?;
            let mut attributes = Vec::with_capacity(attr_len.min(32));
            for _ in 0..attr_len {
                attributes.push(decode_attr(&mut cur, pool)?);
            }
            let default = if cur.bool("param_meta.has_default")? {
                Some(decode_refl_init(&mut cur, pool)?)
            } else {
                None
            };
            let default_constant = cur.opt_str(pool)?;
            let promoted = cur.bool("param_meta.promoted")?;
            parameters.push(reflection::ParameterMetadata {
                attributes,
                default,
                default_constant,
                promoted,
            });
        }
        function_metadata.push(reflection::FunctionMetadata {
            declaration,
            parameters,
        });
    }

    let refl_init_len = cur.len()?;
    let mut reflection_initializers = Vec::with_capacity(refl_init_len.min(256));
    for _ in 0..refl_init_len {
        let span = decode_span(&mut cur)?;
        let first_line = cur.u32()? as usize;
        let function = cur.str(pool)?;
        let namespace = cur.str(pool)?;
        let scope = cur.opt_str(pool)?;
        let imports = if cur.bool("refl_init.has_imports")? {
            let count = cur.len()?;
            let mut map = HashMap::with_capacity(count.min(128));
            for _ in 0..count {
                let kind = decode_refl_name_kind(cur.u8()?)?;
                let key = cur.str(pool)?;
                let val = cur.str(pool)?;
                map.insert((kind, key), val);
            }
            Some(Arc::new(map))
        } else {
            None
        };
        let strict_types = cur.bool("refl_init.strict_types")?;
        reflection_initializers.push(reflection::InitializerSource {
            span,
            first_line,
            function,
            namespace,
            scope,
            imports,
            strict_types,
        });
    }

    let reflection_lines = if cur.bool("unit.has_reflection_lines")? {
        Some(decode_line_table(&mut cur)?)
    } else {
        None
    };

    let reflection_source = if cur.bool("unit.has_reflection_source")? {
        let len = cur.len()?;
        let bytes = cur.take(len)?;
        Some(bytes.into())
    } else {
        None
    };

    let facts = if cur.bool("unit.has_facts")? {
        Some(Arc::new(decode_unit_facts(&mut cur)?))
    } else {
        None
    };

    cur.finish("unit")?;

    Ok(CompiledUnit {
        constant_metadata,
        file,
        declaration_warnings,
        scanner_diagnostics,
        main,
        functions,
        function_metadata,
        has_link_attributes,
        reflection_initializers,
        reflection_lines,
        reflection_source,
        classes,
        hoisted_classes,
        hoisted,
        constants,
        names,
        symbols,
        types,
        metadata: None,
        facts,
    })
}

fn decode_function(cur: &mut Cursor<'_>, pool: &StringPool) -> Result<Function, ImageError> {
    let name = cur.str(pool)?;
    let definition_line = cur.u32()?;
    let registers = cur.u16()?;
    let captures = cur.u16()?;
    let iterators = cur.u16()?;
    let flags = cur.u8()?;
    let return_raw = cur.u32()?;
    let return_type = (return_raw != u32::MAX).then_some(TypeId(return_raw));

    let ins_len = cur.len()?;
    let mut instructions = Vec::with_capacity(ins_len.min(4096));
    for _ in 0..ins_len {
        let raw = cur.take(ENCODED_INSTRUCTION_LEN)?;
        let mut arr = [0u8; ENCODED_INSTRUCTION_LEN];
        arr.copy_from_slice(raw);
        instructions.push(decode_instruction(&arr)?);
    }

    let lines_len = cur.len()?;
    let mut lines = Vec::with_capacity(lines_len.min(4096));
    for _ in 0..lines_len {
        lines.push(cur.u32()?);
    }

    let locals_len = cur.len()?;
    let mut locals = Vec::with_capacity(locals_len.min(512));
    for _ in 0..locals_len {
        locals.push(NameId(cur.u32()?));
    }

    let params_len = cur.len()?;
    let mut parameters = Vec::with_capacity(params_len.min(64));
    for _ in 0..params_len {
        let default_display = cur.opt_str(pool)?;
        let param_name = NameId(cur.u32()?);
        let ty_raw = cur.u32()?;
        let pflags = cur.u8()?;
        parameters.push(Parameter {
            default_display,
            name: param_name,
            ty: (ty_raw != u32::MAX).then_some(TypeId(ty_raw)),
            by_reference: (pflags & 1) != 0,
            variadic: (pflags & 2) != 0,
            has_default: (pflags & 4) != 0,
        });
    }

    let paths_len = cur.len()?;
    let mut paths = Vec::with_capacity(paths_len.min(512));
    for _ in 0..paths_len {
        paths.push(decode_path(cur)?);
    }

    let statics_len = cur.len()?;
    let mut statics = Vec::with_capacity(statics_len.min(128));
    for _ in 0..statics_len {
        statics.push(NameId(cur.u32()?));
    }

    let handlers_len = cur.len()?;
    let mut handlers = Vec::with_capacity(handlers_len.min(128));
    for _ in 0..handlers_len {
        let start = Target(cur.u32()?);
        let end = Target(cur.u32()?);
        let catch_raw = cur.u32()?;
        let finally_raw = cur.u32()?;
        let exception = Reg(cur.u16()?);
        let cleanup_len = cur.len()?;
        let mut cleanup = Vec::with_capacity(cleanup_len.min(64));
        for _ in 0..cleanup_len {
            cleanup.push(Reg(cur.u16()?));
        }
        handlers.push(Handler {
            start,
            end,
            catch: (catch_raw != u32::MAX).then_some(Target(catch_raw)),
            finally: (finally_raw != u32::MAX).then_some(Target(finally_raw)),
            exception,
            cleanup,
        });
    }

    let sites_len = cur.len()?;
    let mut method_sites = Vec::with_capacity(sites_len.min(512));
    for _ in 0..sites_len {
        let receiver = Reg(cur.u16()?);
        let method = NameId(cur.u32()?);
        let first_argument = cur.u32()?;
        let arguments = cur.u16()?;
        let after = Target(cur.u32()?);
        let receiver_taken = cur.bool("method_site.receiver_taken")?;
        method_sites.push(MethodSite {
            receiver,
            method,
            first_argument,
            arguments,
            after,
            receiver_taken,
        });
    }

    let site_args_len = cur.len()?;
    let mut site_arguments = Vec::with_capacity(site_args_len.min(1024));
    for _ in 0..site_args_len {
        let kind = cur.u8()?;
        let reg = Reg(cur.u16()?);
        match kind {
            0 => site_arguments.push(SiteArgument::Local(reg)),
            1 => site_arguments.push(SiteArgument::Value(reg)),
            other => {
                return Err(ImageError::Corrupt(format!(
                    "invalid SiteArgument tag {other}"
                )));
            }
        }
    }

    Ok(Function {
        name,
        definition_line,
        instructions: instructions.into(),
        lines,
        registers,
        locals,
        parameters,
        captures,
        return_type,
        returns_reference: (flags & 1) != 0,
        is_generator: (flags & 2) != 0,
        paths,
        statics,
        iterators,
        dynamic_scope: (flags & 4) != 0,
        observes_arguments: (flags & 8) != 0,
        argument_layout: if (flags & 32) != 0 {
            ArgumentLayout::Variadic
        } else {
            ArgumentLayout::Fixed
        },
        strict_types: (flags & 16) != 0,
        handlers,
        method_sites,
        site_arguments,
    })
}

fn decode_path(cur: &mut Cursor<'_>) -> Result<BcPath, ImageError> {
    let base = match cur.u8()? {
        0 => PathBase::This {
            scratch: Reg(cur.u16()?),
        },
        1 => PathBase::StaticProperty {
            class: Reg(cur.u16()?),
            name: Reg(cur.u16()?),
            scratch: Reg(cur.u16()?),
        },
        2 => PathBase::NamedStaticProperty {
            class: SymbolId(cur.u32()?),
            name: NameId(cur.u32()?),
            scratch: Reg(cur.u16()?),
        },
        3 => PathBase::Register(Reg(cur.u16()?)),
        4 => PathBase::Named(Reg(cur.u16()?)),
        5 => PathBase::GlobalNamed {
            name: Reg(cur.u16()?),
            scratch: Reg(cur.u16()?),
        },
        6 => PathBase::Globals,
        other => return Err(ImageError::Corrupt(format!("invalid PathBase {other}"))),
    };
    let steps_len = cur.len()?;
    let mut steps = Vec::with_capacity(steps_len.min(32));
    for _ in 0..steps_len {
        let step = match cur.u8()? {
            0 => Step::Property(NameId(cur.u32()?)),
            1 => Step::DynamicProperty(Reg(cur.u16()?)),
            2 => Step::Index(Reg(cur.u16()?)),
            3 => Step::Append,
            other => return Err(ImageError::Corrupt(format!("invalid Step {other}"))),
        };
        steps.push(step);
    }
    Ok(BcPath { base, steps })
}

fn decode_constant(
    cur: &mut Cursor<'_>,
    pool: &StringPool,
    depth: usize,
) -> Result<Constant, ImageError> {
    if depth > 256 {
        return Err(ImageError::Corrupt(
            "constant array nesting depth exceeded".into(),
        ));
    }
    match cur.u8()? {
        0 => Ok(Constant::Null),
        1 => Ok(Constant::Bool(cur.bool("constant.bool")?)),
        2 => Ok(Constant::Int(cur.i64()?)),
        3 => Ok(Constant::Float(f64::from_bits(cur.u64()?))),
        4 => Ok(Constant::String(cur.pool_bytes(pool)?)),
        5 => {
            let len = cur.len()?;
            let mut items = Vec::with_capacity(len.min(1024));
            for _ in 0..len {
                let key = match cur.u8()? {
                    0 => ConstantKey::Next,
                    1 => ConstantKey::Int(cur.i64()?),
                    2 => ConstantKey::String(cur.pool_bytes(pool)?),
                    other => {
                        return Err(ImageError::Corrupt(format!("invalid ConstantKey {other}")));
                    }
                };
                let value = decode_constant(cur, pool, depth + 1)?;
                items.push((key, value));
            }
            Ok(Constant::Array(items))
        }
        other => Err(ImageError::Corrupt(format!("invalid Constant tag {other}"))),
    }
}

fn decode_class(cur: &mut Cursor<'_>, pool: &StringPool) -> Result<ClassDeclaration, ImageError> {
    let metadata = decode_decl_meta(cur, pool)?;
    let line = cur.u32()?;
    let name = cur.str(pool)?;
    let parent_raw = cur.u32()?;
    let parent = (parent_raw != u32::MAX).then_some(SymbolId(parent_raw));
    let iface_len = cur.len()?;
    let mut interfaces = Vec::with_capacity(iface_len.min(64));
    for _ in 0..iface_len {
        interfaces.push(SymbolId(cur.u32()?));
    }
    let kind = decode_class_kind(cur.u8()?)?;
    let flags = cur.u8()?;
    let enum_backing = match cur.u8()? {
        0 => None,
        1 => Some(EnumBacking::Int),
        2 => Some(EnumBacking::String),
        other => {
            return Err(ImageError::Corrupt(format!("invalid EnumBacking {other}")));
        }
    };

    let const_len = cur.len()?;
    let mut constants = Vec::with_capacity(const_len.min(128));
    for _ in 0..const_len {
        let cmeta = decode_decl_meta(cur, pool)?;
        let cname = NameId(cur.u32()?);
        let visibility = decode_visibility(cur.u8()?)?;
        let is_final = cur.bool("class_const.is_final")?;
        let initializer = FunctionId(cur.u32()?);
        let ty_raw = cur.u32()?;
        constants.push(ClassConstant {
            metadata: cmeta,
            name: cname,
            visibility,
            is_final,
            initializer,
            ty: (ty_raw != u32::MAX).then_some(TypeId(ty_raw)),
        });
    }

    let prop_len = cur.len()?;
    let mut properties = Vec::with_capacity(prop_len.min(128));
    for _ in 0..prop_len {
        let pmeta = decode_decl_meta(cur, pool)?;
        let pname = NameId(cur.u32()?);
        let visibility = decode_visibility(cur.u8()?)?;
        let set_vis_raw = cur.u8()?;
        let set_visibility = if set_vis_raw == u8::MAX {
            None
        } else {
            Some(decode_visibility(set_vis_raw)?)
        };
        let pflags = cur.u8()?;
        let ty_raw = cur.u32()?;
        let default = match cur.u8()? {
            0 => None,
            1 => Some(Initializer::Literal(ConstId(cur.u32()?))),
            2 => Some(Initializer::Function(FunctionId(cur.u32()?))),
            other => {
                return Err(ImageError::Corrupt(format!(
                    "invalid property default tag {other}"
                )));
            }
        };
        let hooks = if cur.bool("property.has_hooks")? {
            let is_virtual = cur.bool("property_hooks.is_virtual")?;
            let get = decode_opt_hook(cur)?;
            let set = decode_opt_hook(cur)?;
            Some(Box::new(PropertyHooks {
                get,
                set,
                is_virtual,
            }))
        } else {
            None
        };
        properties.push(PropertyDeclaration {
            set_visibility,
            is_final: (pflags & 1) != 0,
            promoted: (pflags & 2) != 0,
            metadata: pmeta,
            name: pname,
            visibility,
            is_static: (pflags & 4) != 0,
            is_readonly: (pflags & 8) != 0,
            ty: (ty_raw != u32::MAX).then_some(TypeId(ty_raw)),
            default,
            hooks,
        });
    }

    let method_len = cur.len()?;
    let mut methods = Vec::with_capacity(method_len.min(256));
    for _ in 0..method_len {
        let mname = NameId(cur.u32()?);
        let function = FunctionId(cur.u32()?);
        let visibility = decode_visibility(cur.u8()?)?;
        let mflags = cur.u8()?;
        methods.push(MethodDeclaration {
            name: mname,
            function,
            visibility,
            is_static: (mflags & 1) != 0,
            is_abstract: (mflags & 2) != 0,
            is_final: (mflags & 4) != 0,
        });
    }

    let trait_len = cur.len()?;
    let mut traits = Vec::with_capacity(trait_len.min(64));
    for _ in 0..trait_len {
        let names = cur.str_vec(pool)?;
        let adapt_len = cur.len()?;
        let mut adaptations = Vec::with_capacity(adapt_len.min(32));
        for _ in 0..adapt_len {
            match cur.u8()? {
                0 => {
                    let trait_name = cur.str(pool)?;
                    let method = cur.str(pool)?;
                    let instead_of = cur.str_vec(pool)?;
                    adaptations.push(TraitAdaptation::Precedence {
                        trait_name,
                        method,
                        instead_of,
                    });
                }
                1 => {
                    let trait_name = cur.opt_str(pool)?;
                    let method = cur.str(pool)?;
                    let alias = cur.opt_str(pool)?;
                    let vis_raw = cur.u8()?;
                    let visibility = if vis_raw == u8::MAX {
                        None
                    } else {
                        Some(decode_visibility(vis_raw)?)
                    };
                    let is_final = cur.bool("trait_alias.is_final")?;
                    adaptations.push(TraitAdaptation::Alias {
                        trait_name,
                        method,
                        alias,
                        visibility,
                        is_final,
                    });
                }
                other => {
                    return Err(ImageError::Corrupt(format!(
                        "invalid TraitAdaptation tag {other}"
                    )));
                }
            }
        }
        traits.push(TraitUse { names, adaptations });
    }

    let case_len = cur.len()?;
    let mut enum_cases = Vec::with_capacity(case_len.min(128));
    for _ in 0..case_len {
        let cmeta = decode_decl_meta(cur, pool)?;
        let cname = NameId(cur.u32()?);
        let backing_raw = cur.u32()?;
        enum_cases.push(EnumCase {
            metadata: cmeta,
            name: cname,
            backing: (backing_raw != u32::MAX).then_some(FunctionId(backing_raw)),
        });
    }

    Ok(ClassDeclaration {
        metadata,
        line,
        name,
        parent,
        interfaces,
        kind,
        is_abstract: (flags & 1) != 0,
        is_final: (flags & 2) != 0,
        is_readonly: (flags & 4) != 0,
        allows_dynamic_properties: (flags & 8) != 0,
        constants,
        properties,
        methods,
        traits,
        enum_backing,
        enum_cases,
    })
}

fn decode_opt_hook(cur: &mut Cursor<'_>) -> Result<Option<PropertyHook<FunctionId>>, ImageError> {
    if !cur.bool("property_hook.present")? {
        return Ok(None);
    }
    let function = FunctionId(cur.u32()?);
    let flags = cur.u8()?;
    Ok(Some(PropertyHook {
        function,
        is_abstract: (flags & 1) != 0,
        is_final: (flags & 2) != 0,
    }))
}

fn decode_decl_meta(
    cur: &mut Cursor<'_>,
    pool: &StringPool,
) -> Result<reflection::DeclarationMetadata, ImageError> {
    let span = decode_span(cur)?;
    let doc_comment = decode_span(cur)?;
    let attr_len = cur.len()?;
    let mut attributes = Vec::with_capacity(attr_len.min(32));
    for _ in 0..attr_len {
        attributes.push(decode_attr(cur, pool)?);
    }
    Ok(reflection::DeclarationMetadata {
        span,
        doc_comment,
        attributes,
    })
}

fn decode_attr(
    cur: &mut Cursor<'_>,
    pool: &StringPool,
) -> Result<reflection::Attribute, ImageError> {
    let name = cur.str(pool)?;
    let line = cur.u32()? as usize;
    let strict_types = cur.bool("attr.strict_types")?;
    let validation_error = cur.opt_str(pool)?;
    let arg_len = cur.len()?;
    let mut arguments = Vec::with_capacity(arg_len.min(32));
    for _ in 0..arg_len {
        let arg_name = cur.opt_str(pool)?;
        let initializer = decode_refl_init(cur, pool)?;
        arguments.push(reflection::AttributeArgument {
            name: arg_name,
            initializer,
        });
    }
    Ok(reflection::Attribute {
        name,
        line,
        strict_types,
        validation_error,
        arguments,
    })
}

fn decode_refl_init(
    cur: &mut Cursor<'_>,
    pool: &StringPool,
) -> Result<reflection::Initializer, ImageError> {
    match cur.u8()? {
        0 => Ok(reflection::Initializer::Null),
        1 => Ok(reflection::Initializer::Bool(cur.bool("refl_init.bool")?)),
        2 => Ok(reflection::Initializer::Int(cur.i64()?)),
        3 => Ok(reflection::Initializer::Float(cur.u64()?)),
        4 => Ok(reflection::Initializer::String(cur.pool_bytes(pool)?)),
        5 => Ok(reflection::Initializer::EmptyArray),
        6 => Ok(reflection::Initializer::Expression(
            reflection::InitializerId(cur.u32()? as usize),
        )),
        other => Err(ImageError::Corrupt(format!(
            "invalid reflection Initializer tag {other}"
        ))),
    }
}

fn decode_line_table(cur: &mut Cursor<'_>) -> Result<LineTable, ImageError> {
    let first_line = cur.u32()? as usize;
    let len = cur.len()?;
    let mut starts = Vec::with_capacity(len.min(4096));
    for _ in 0..len {
        let val = usize::try_from(cur.u64()?)
            .map_err(|_| ImageError::Corrupt("LineTable start exceeds usize".into()))?;
        starts.push(val);
    }
    LineTable::from_parts(starts, first_line)
        .ok_or_else(|| ImageError::Corrupt("invalid LineTable starts".into()))
}

fn decode_span(cur: &mut Cursor<'_>) -> Result<Span, ImageError> {
    let start = usize::try_from(cur.u64()?)
        .map_err(|_| ImageError::Corrupt("span start exceeds usize".into()))?;
    let end = usize::try_from(cur.u64()?)
        .map_err(|_| ImageError::Corrupt("span end exceeds usize".into()))?;
    Ok(Span::new(start, end))
}

fn decode_program_facts(raw: &[u8], pool: &StringPool) -> Result<ProgramFacts, ImageError> {
    let mut cur = Cursor::new(raw);
    let mode = decode_build_mode(cur.u8()?)?;
    let unit_len = cur.len()?;
    let mut units = Vec::with_capacity(unit_len.min(4096));
    for _ in 0..unit_len {
        units.push(cur.str(pool)?.into_boxed_str());
    }
    let cls_len = cur.len()?;
    let mut classes = Vec::with_capacity(cls_len.min(4096));
    for _ in 0..cls_len {
        classes.push(decode_class_layout(&mut cur, pool)?);
    }
    let desc_len = cur.len()?;
    let mut descendants = Vec::with_capacity(desc_len.min(4096));
    for _ in 0..desc_len {
        descendants.push(ClassRef(cur.u32()?));
    }
    let fn_len = cur.len()?;
    let mut functions = Vec::with_capacity(fn_len.min(4096));
    for _ in 0..fn_len {
        let name = cur.str(pool)?.into_boxed_str();
        let decl = decode_decl_uniqueness(&mut cur)?;
        let unit = UnitRef(cur.u32()?);
        let function = cur.u32()?;
        functions.push((name, decl, FunctionRef { unit, function }));
    }
    cur.finish("FACTS")?;
    Ok(ProgramFacts {
        mode,
        units: units.into_boxed_slice(),
        classes: classes.into_boxed_slice(),
        descendants: descendants.into_boxed_slice(),
        functions: functions.into_boxed_slice(),
        audit: DynamicAudit::default(),
    })
}

fn decode_class_layout(
    cur: &mut Cursor<'_>,
    pool: &StringPool,
) -> Result<AotClassLayout, ImageError> {
    let name = cur.str(pool)?.into_boxed_str();
    let decl = decode_decl_uniqueness(cur)?;
    let kind = decode_class_kind(cur.u8()?)?;
    let parent_raw = cur.u32()?;
    let parent = (parent_raw != u32::MAX).then_some(ClassRef(parent_raw));
    let anc_len = cur.len()?;
    let mut ancestors = Vec::with_capacity(anc_len.min(64));
    for _ in 0..anc_len {
        ancestors.push(ClassRef(cur.u32()?));
    }
    let first_descendant = cur.u32()?;
    let descendants = cur.u32()?;
    let flags = ClassFlags(cur.u16()?);
    let slot_len = cur.len()?;
    let mut slots = Vec::with_capacity(slot_len.min(128));
    for _ in 0..slot_len {
        let key = cur.pool_bytes(pool)?.into_boxed_slice();
        let declaring = ClassRef(cur.u32()?);
        let sflags = PropFlags(cur.u8()?);
        let ty = decode_sound_type(cur)?;
        slots.push(AotPropSlot {
            key,
            declaring,
            flags: sflags,
            ty,
        });
    }
    let method_len = cur.len()?;
    let mut methods = Vec::with_capacity(method_len.min(256));
    for _ in 0..method_len {
        let mname = cur.str(pool)?.into_boxed_str();
        let unit = UnitRef(cur.u32()?);
        let function = cur.u32()?;
        let mflags = MethodFlags(cur.u8()?);
        methods.push(AotMethod {
            name: mname,
            function: FunctionRef { unit, function },
            flags: mflags,
        });
    }
    Ok(AotClassLayout {
        name,
        decl,
        kind,
        parent,
        ancestors: ancestors.into_boxed_slice(),
        first_descendant,
        descendants,
        flags,
        slots: slots.into_boxed_slice(),
        methods: methods.into_boxed_slice(),
    })
}

fn decode_unit_facts(cur: &mut Cursor<'_>) -> Result<UnitBuildFacts, ImageError> {
    let unit = UnitRef(cur.u32()?);
    let cls_len = cur.len()?;
    let mut classes = Vec::with_capacity(cls_len.min(256));
    for _ in 0..cls_len {
        classes.push(ClassRef(cur.u32()?));
    }
    let fn_len = cur.len()?;
    let mut functions = Vec::with_capacity(fn_len.min(1024));
    for _ in 0..fn_len {
        functions.push(decode_function_facts(cur)?);
    }
    Ok(UnitBuildFacts {
        unit,
        classes: classes.into_boxed_slice(),
        functions: functions.into_boxed_slice(),
    })
}

fn decode_function_facts(cur: &mut Cursor<'_>) -> Result<FunctionBuildFacts, ImageError> {
    let flags = FunctionFlags(cur.u8()?);
    let locals_len = cur.len()?;
    let mut locals = Vec::with_capacity(locals_len.min(512));
    for _ in 0..locals_len {
        locals.push(LocalFlags(cur.u8()?));
    }
    let blocks_len = cur.len()?;
    let mut block_starts = Vec::with_capacity(blocks_len.min(512));
    for _ in 0..blocks_len {
        block_starts.push(cur.u32()?);
    }
    let bt_len = cur.len()?;
    let mut block_types = Vec::with_capacity(bt_len.min(1024));
    for _ in 0..bt_len {
        let block = cur.u16()?;
        let reg = Reg(cur.u16()?);
        let ty = decode_sound_type(cur)?;
        block_types.push((block, reg, ty));
    }
    let calls_len = cur.len()?;
    let mut calls = Vec::with_capacity(calls_len.min(512));
    for _ in 0..calls_len {
        let at = cur.u32()?;
        let unit = UnitRef(cur.u32()?);
        let function = cur.u32()?;
        let needs = Assumptions(cur.u8()?);
        let cflags = CallFlags(cur.u8()?);
        let result = decode_sound_type(cur)?;
        calls.push(ResolvedCall {
            at,
            target: FunctionRef { unit, function },
            needs,
            flags: cflags,
            result,
        });
    }
    let props_len = cur.len()?;
    let mut props = Vec::with_capacity(props_len.min(512));
    for _ in 0..props_len {
        let at = cur.u32()?;
        let class = ClassRef(cur.u32()?);
        let slot = cur.u16()?;
        let needs = Assumptions(cur.u8()?);
        let pflags = PropAccess(cur.u8()?);
        let ty = decode_sound_type(cur)?;
        props.push(ResolvedProp {
            at,
            class,
            slot,
            needs,
            flags: pflags,
            ty,
        });
    }
    let returns = decode_sound_type(cur)?;
    Ok(FunctionBuildFacts {
        flags,
        locals: locals.into_boxed_slice(),
        block_starts: block_starts.into_boxed_slice(),
        block_types: block_types.into_boxed_slice(),
        calls: calls.into_boxed_slice(),
        props: props.into_boxed_slice(),
        returns,
    })
}

fn decode_sound_type(cur: &mut Cursor<'_>) -> Result<SoundType, ImageError> {
    let mask = SoundMask(cur.u16()?);
    let class = match cur.u8()? {
        0 => {
            let _ = cur.u32()?;
            ClassBound::Any
        }
        1 => ClassBound::Exact(ClassRef(cur.u32()?)),
        2 => ClassBound::Within(ClassRef(cur.u32()?)),
        other => return Err(ImageError::Corrupt(format!("invalid ClassBound {other}"))),
    };
    Ok(SoundType { mask, class })
}

fn decode_decl_uniqueness(cur: &mut Cursor<'_>) -> Result<DeclUniqueness, ImageError> {
    match cur.u8()? {
        0 => Ok(DeclUniqueness::Unique {
            unit: UnitRef(cur.u32()?),
            hoisted: cur.bool("decl.hoisted")?,
        }),
        1 => Ok(DeclUniqueness::Duplicate),
        2 => Ok(DeclUniqueness::Native),
        3 => Ok(DeclUniqueness::Unknown),
        other => Err(ImageError::Corrupt(format!(
            "invalid DeclUniqueness {other}"
        ))),
    }
}

fn decode_audit(raw: &[u8], pool: &StringPool) -> Result<DynamicAudit, ImageError> {
    let mut cur = Cursor::new(raw);
    let holds = Assumptions(cur.u8()?);
    let count = cur.len()?;
    let mut findings = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let kind = decode_dynamic_kind(cur.u8()?)?;
        let radius = match cur.u8()? {
            0 => BlastRadius::Function,
            1 => BlastRadius::Class,
            2 => BlastRadius::Names,
            other => {
                return Err(ImageError::Corrupt(format!("invalid BlastRadius {other}")));
            }
        };
        let unit = UnitRef(cur.u32()?);
        let function = cur.u32()?;
        let line = cur.u32()?;
        let detail = cur.str(pool)?.into_boxed_str();
        findings.push(DynamicFinding {
            kind,
            radius,
            at: FunctionRef { unit, function },
            line,
            detail,
        });
    }
    cur.finish("AUDIT")?;
    Ok(DynamicAudit {
        findings: findings.into_boxed_slice(),
        holds,
    })
}

fn decode_build_mode(raw: u8) -> Result<BuildMode, ImageError> {
    match raw {
        0 => Ok(BuildMode::Hybrid),
        1 => Ok(BuildMode::Strict),
        other => Err(ImageError::Corrupt(format!("invalid BuildMode {other}"))),
    }
}

fn decode_dynamic_kind(raw: u8) -> Result<DynamicKind, ImageError> {
    match raw {
        0 => Ok(DynamicKind::Eval),
        1 => Ok(DynamicKind::DynamicInclude),
        2 => Ok(DynamicKind::DynamicNew),
        3 => Ok(DynamicKind::DynamicStaticCall),
        4 => Ok(DynamicKind::DynamicMethodCall),
        5 => Ok(DynamicKind::ClassAlias),
        6 => Ok(DynamicKind::Extract),
        7 => Ok(DynamicKind::Compact),
        8 => Ok(DynamicKind::Unserialize),
        9 => Ok(DynamicKind::Reflection),
        10 => Ok(DynamicKind::VariableVariable),
        other => Err(ImageError::Corrupt(format!("invalid DynamicKind {other}"))),
    }
}

fn decode_class_kind(raw: u8) -> Result<ClassKind, ImageError> {
    match raw {
        0 => Ok(ClassKind::Class),
        1 => Ok(ClassKind::Interface),
        2 => Ok(ClassKind::Anonymous),
        3 => Ok(ClassKind::Trait),
        4 => Ok(ClassKind::Enum),
        other => Err(ImageError::Corrupt(format!("invalid ClassKind {other}"))),
    }
}

fn decode_visibility(raw: u8) -> Result<Visibility, ImageError> {
    match raw {
        0 => Ok(Visibility::Public),
        1 => Ok(Visibility::Protected),
        2 => Ok(Visibility::Private),
        other => Err(ImageError::Corrupt(format!("invalid Visibility {other}"))),
    }
}

fn decode_refl_name_kind(raw: u8) -> Result<reflection::NameKind, ImageError> {
    match raw {
        0 => Ok(reflection::NameKind::Class),
        1 => Ok(reflection::NameKind::Function),
        2 => Ok(reflection::NameKind::Constant),
        other => Err(ImageError::Corrupt(format!(
            "invalid reflection NameKind {other}"
        ))),
    }
}

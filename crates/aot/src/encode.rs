//! Deterministic `.phb` binary encoder.

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use phera_bytecode::facts::{
    AotClassLayout, AotMethod, AotPropSlot, BuildMode, ClassBound, DeclUniqueness, DynamicAudit,
    DynamicFinding, DynamicKind, FunctionBuildFacts, ProgramFacts, ResolvedCall, ResolvedProp,
    SoundType, UnitBuildFacts,
};
use phera_bytecode::{
    ArgumentLayout, ClassConstant, ClassDeclaration, ClassKind, CompiledUnit, Constant,
    ConstantKey, EnumBacking, EnumCase, Function, Handler, Initializer, MethodDeclaration,
    Parameter, Path as BcPath, PathBase, PropertyDeclaration, PropertyHook, PropertyHooks,
    SiteArgument, Step, Symbol, TraitAdaptation, TraitUse, Type, Visibility, reflection,
};
use phera_source::{CompileDiagnostic, CompileDiagnosticKind, LineTable, Span};

use crate::error::ImageError;
use crate::format::{
    FORMAT_VERSION, HEADER_LEN, ImageClassEntry, ImageFileEntry, ImageFlags, ImageManifest, MAGIC,
    SECTION_ENTRY_LEN, SectionKind, UNIT_ENTRY_LEN, blake2b_64, encode_allow_mask,
    header_body_checksum,
};
use crate::opcodes::encode_instruction;
use crate::strings::{StringId, StringPoolBuilder};

/// Inputs needed to emit a deterministic `.phb` production image.
pub struct ImageInput {
    pub runtime_abi_hash: [u8; 16],
    pub manifest: ImageManifest,
    pub files: Vec<ImageFileEntry>,
    pub classmap: Vec<ImageClassEntry>,
    pub units: Vec<Arc<CompiledUnit>>,
    pub program_facts: Option<ProgramFacts>,
    pub audit: DynamicAudit,
}

/// Encodes `input` into a deterministic `.phb` byte buffer.
pub fn encode_image(input: &ImageInput) -> Result<Vec<u8>, ImageError> {
    let mut strings = StringPoolBuilder::default();

    let manifest_bytes = encode_manifest(&input.manifest, &mut strings)?;
    let files_bytes = encode_files(&input.files, &mut strings)?;
    let classmap_bytes = encode_classmap(&input.classmap, &mut strings)?;
    let units_bytes = encode_units(&input.units, &mut strings)?;
    let facts_bytes = match &input.program_facts {
        Some(facts) => Some(encode_program_facts(facts, &mut strings)?),
        None => None,
    };
    let audit_bytes = encode_audit(&input.audit, &mut strings)?;
    let strings_bytes = strings.encode()?;

    let mut sections: Vec<(SectionKind, Vec<u8>)> = vec![
        (SectionKind::Manifest, manifest_bytes),
        (SectionKind::Strings, strings_bytes),
        (SectionKind::Files, files_bytes),
        (SectionKind::Classmap, classmap_bytes),
        (SectionKind::Units, units_bytes),
    ];
    if let Some(facts) = facts_bytes {
        sections.push((SectionKind::Facts, facts));
    }
    sections.push((SectionKind::Audit, audit_bytes));

    let section_count = u32::try_from(sections.len())
        .map_err(|_| ImageError::Encode("too many sections".into()))?;
    let dir_len = sections
        .len()
        .checked_mul(SECTION_ENTRY_LEN)
        .ok_or_else(|| ImageError::Encode("directory size overflow".into()))?;
    let body_payload_len: usize = sections
        .iter()
        .try_fold(0usize, |acc, (_, data)| acc.checked_add(data.len()))
        .ok_or_else(|| ImageError::Encode("image payload size overflow".into()))?;
    let total_len = (HEADER_LEN as usize)
        .checked_add(dir_len)
        .and_then(|n| n.checked_add(body_payload_len))
        .ok_or_else(|| ImageError::Encode("image file length overflow".into()))?;

    let mut body = Vec::with_capacity(dir_len + body_payload_len);
    let mut cursor = u64::from(HEADER_LEN) + dir_len as u64;
    for (kind, data) in &sections {
        let len = data.len() as u64;
        let check = blake2b_64(data);
        write_u32(&mut body, *kind as u32);
        write_u32(&mut body, 0);
        write_u64(&mut body, cursor);
        write_u64(&mut body, len);
        write_u64(&mut body, check);
        cursor = cursor
            .checked_add(len)
            .ok_or_else(|| ImageError::Encode("section offset overflow".into()))?;
    }
    for (_, data) in &sections {
        body.extend_from_slice(data);
    }

    let mut flags = ImageFlags::NONE;
    if input.manifest.mode == BuildMode::Strict {
        flags = flags.union(ImageFlags::STRICT);
    }
    if input.program_facts.is_some() {
        flags = flags.union(ImageFlags::HAS_FACTS);
    }
    if input.manifest.has_web {
        flags = flags.union(ImageFlags::WEB_UNITS);
    }
    if input.manifest.has_cli {
        flags = flags.union(ImageFlags::CLI_UNITS);
    }
    if input.manifest.allow.contains(&DynamicKind::Eval) {
        flags = flags.union(ImageFlags::ALLOW_EVAL);
    }
    if input.manifest.allow.contains(&DynamicKind::ClassAlias) {
        flags = flags.union(ImageFlags::ALLOW_CLASS_ALIAS);
    }

    let mut header_before = Vec::with_capacity(32);
    header_before.extend_from_slice(&MAGIC);
    write_u16(&mut header_before, FORMAT_VERSION);
    write_u16(&mut header_before, flags.0);
    write_u32(&mut header_before, HEADER_LEN);
    header_before.extend_from_slice(&input.runtime_abi_hash);
    debug_assert_eq!(header_before.len(), 32);

    let mut header_after = Vec::with_capacity(16);
    write_u64(&mut header_after, total_len as u64);
    write_u32(&mut header_after, section_count);
    write_u32(&mut header_after, 0);
    debug_assert_eq!(header_after.len(), 16);

    let body_checksum = header_body_checksum(&header_before, &header_after, &body);

    let mut out = Vec::with_capacity(total_len);
    out.extend_from_slice(&header_before);
    out.extend_from_slice(&body_checksum);
    out.extend_from_slice(&header_after);
    debug_assert_eq!(out.len(), HEADER_LEN as usize);
    out.extend_from_slice(&body);
    Ok(out)
}

/// Atomically writes already-encoded `.phb` bytes to `path` (creating parent
/// directories and removing the temporary file if writing or renaming fails).
pub fn write_encoded_image(path: &Path, bytes: &[u8]) -> Result<usize, ImageError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("phb.tmp.{}", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, path)?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(ImageError::Io(error));
    }
    Ok(bytes.len())
}

/// Atomically encodes and writes a `.phb` image to `path` (creating parent directories).
pub fn write_image(path: &Path, input: &ImageInput) -> Result<usize, ImageError> {
    let bytes = encode_image(input)?;
    write_encoded_image(path, &bytes)
}

fn encode_manifest(
    manifest: &ImageManifest,
    strings: &mut StringPoolBuilder,
) -> Result<Vec<u8>, ImageError> {
    let mut out = Vec::new();
    write_str_id(&mut out, strings.intern_str(&manifest.project_root)?);
    write_u8(
        &mut out,
        match manifest.mode {
            BuildMode::Hybrid => 0,
            BuildMode::Strict => 1,
        },
    );
    let target_bits = u8::from(manifest.has_web) | (u8::from(manifest.has_cli) << 1);
    write_u8(&mut out, target_bits);
    write_u8(&mut out, u8::from(manifest.include_dev));
    write_u8(&mut out, 0);
    write_u32(&mut out, encode_allow_mask(&manifest.allow));
    write_opt_str(&mut out, manifest.framework.as_deref(), strings)?;
    write_str_vec(&mut out, &manifest.entries, strings)?;
    write_str_vec(&mut out, &manifest.include_globs, strings)?;
    write_str_vec(&mut out, &manifest.exclude_globs, strings)?;
    Ok(out)
}

fn encode_files(
    files: &[ImageFileEntry],
    strings: &mut StringPoolBuilder,
) -> Result<Vec<u8>, ImageError> {
    let mut sorted = files.to_vec();
    sorted.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    let mut out = Vec::new();
    write_len(&mut out, sorted.len())?;
    for entry in &sorted {
        write_str_id(&mut out, strings.intern_str(&entry.relative_path)?);
        write_str_id(&mut out, strings.intern_str(&entry.canonical_path)?);
        write_u32(&mut out, entry.unit_web.unwrap_or(u32::MAX));
        write_u32(&mut out, entry.unit_cli.unwrap_or(u32::MAX));
        write_u64(&mut out, entry.source_len);
        out.extend_from_slice(&entry.source_hash);
        write_u32(&mut out, entry.flags.0);
    }
    Ok(out)
}

fn encode_classmap(
    entries: &[ImageClassEntry],
    strings: &mut StringPoolBuilder,
) -> Result<Vec<u8>, ImageError> {
    let mut sorted = entries.to_vec();
    sorted.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| a.file_index.cmp(&b.file_index))
            .then_with(|| a.class_id.cmp(&b.class_id))
    });
    let mut out = Vec::new();
    write_len(&mut out, sorted.len())?;
    for entry in &sorted {
        write_str_id(&mut out, strings.intern_str(&entry.name)?);
        write_u32(&mut out, entry.file_index);
        write_u32(&mut out, entry.class_id);
        write_u32(&mut out, entry.class_ref.unwrap_or(u32::MAX));
    }
    Ok(out)
}

fn encode_units(
    units: &[Arc<CompiledUnit>],
    strings: &mut StringPoolBuilder,
) -> Result<Vec<u8>, ImageError> {
    let mut blobs = Vec::with_capacity(units.len());
    for unit in units {
        blobs.push(encode_unit(unit, strings)?);
    }
    let dir_bytes = 4usize
        .checked_add(
            blobs
                .len()
                .checked_mul(UNIT_ENTRY_LEN)
                .ok_or_else(|| ImageError::Encode("unit directory overflow".into()))?,
        )
        .ok_or_else(|| ImageError::Encode("unit directory header overflow".into()))?;
    let payload_bytes: usize = blobs
        .iter()
        .try_fold(0usize, |acc, b| acc.checked_add(b.len()))
        .ok_or_else(|| ImageError::Encode("unit payload overflow".into()))?;
    let mut out = Vec::with_capacity(dir_bytes + payload_bytes);
    write_len(&mut out, blobs.len())?;
    let mut rel_offset = dir_bytes as u64;
    for blob in &blobs {
        let len = u32::try_from(blob.len())
            .map_err(|_| ImageError::Encode("unit blob exceeds 4 GiB".into()))?;
        let check = blake2b_64(blob);
        write_u64(&mut out, rel_offset);
        write_u32(&mut out, len);
        write_u32(&mut out, 0);
        write_u64(&mut out, check);
        rel_offset = rel_offset
            .checked_add(u64::from(len))
            .ok_or_else(|| ImageError::Encode("unit offset overflow".into()))?;
    }
    for blob in &blobs {
        out.extend_from_slice(blob);
    }
    Ok(out)
}

fn encode_unit(
    unit: &CompiledUnit,
    strings: &mut StringPoolBuilder,
) -> Result<Vec<u8>, ImageError> {
    let mut out = Vec::new();
    write_str_id(&mut out, strings.intern_str(&unit.file)?);
    write_u8(&mut out, u8::from(unit.has_link_attributes));

    // Constant metadata (sorted deterministically by key).
    let mut const_meta: Vec<(&String, &reflection::DeclarationMetadata)> =
        unit.constant_metadata.iter().collect();
    const_meta.sort_by_key(|(a, _)| *a);
    write_len(&mut out, const_meta.len())?;
    for (name, meta) in const_meta {
        write_str_id(&mut out, strings.intern_str(name)?);
        encode_decl_meta(&mut out, meta, strings)?;
    }

    // Declaration warnings.
    write_len(&mut out, unit.declaration_warnings.len())?;
    for warning in &unit.declaration_warnings {
        write_str_id(&mut out, strings.intern_str(&warning.message)?);
        write_u32(&mut out, warning.line);
    }

    // Scanner diagnostics.
    write_len(&mut out, unit.scanner_diagnostics.len())?;
    for diag in &unit.scanner_diagnostics {
        encode_compile_diagnostic(&mut out, diag, strings)?;
    }

    // Tables needed by functions/classes.
    write_len(&mut out, unit.names.len())?;
    for name in &unit.names {
        write_str_id(&mut out, strings.intern_str(name)?);
    }

    write_len(&mut out, unit.symbols.len())?;
    for sym in &unit.symbols {
        encode_symbol(&mut out, sym, strings)?;
    }

    write_len(&mut out, unit.types.len())?;
    for ty in &unit.types {
        encode_type(&mut out, ty, strings)?;
    }

    write_len(&mut out, unit.constants.len())?;
    for constant in &unit.constants {
        encode_constant(&mut out, constant, strings)?;
    }

    // Main + functions.
    encode_function(&mut out, &unit.main, strings)?;
    write_len(&mut out, unit.functions.len())?;
    for func in &unit.functions {
        encode_function(&mut out, func, strings)?;
    }

    // Hoisted tables.
    write_len(&mut out, unit.hoisted.len())?;
    for id in &unit.hoisted {
        write_u32(&mut out, id.0);
    }
    write_len(&mut out, unit.hoisted_classes.len())?;
    for id in &unit.hoisted_classes {
        write_u32(&mut out, id.0);
    }

    // Classes.
    write_len(&mut out, unit.classes.len())?;
    for class in &unit.classes {
        encode_class(&mut out, class, strings)?;
    }

    // Reflection metadata & cold source.
    write_len(&mut out, unit.function_metadata.len())?;
    for meta in &unit.function_metadata {
        encode_decl_meta(&mut out, &meta.declaration, strings)?;
        write_len(&mut out, meta.parameters.len())?;
        for param in &meta.parameters {
            write_len(&mut out, param.attributes.len())?;
            for attr in &param.attributes {
                encode_attr(&mut out, attr, strings)?;
            }
            match &param.default {
                Some(init) => {
                    write_u8(&mut out, 1);
                    encode_refl_init(&mut out, init, strings)?;
                }
                None => write_u8(&mut out, 0),
            }
            write_opt_str(&mut out, param.default_constant.as_deref(), strings)?;
            write_u8(&mut out, u8::from(param.promoted));
        }
    }

    write_len(&mut out, unit.reflection_initializers.len())?;
    for init in &unit.reflection_initializers {
        encode_span(&mut out, init.span)?;
        write_u32(
            &mut out,
            u32::try_from(init.first_line)
                .map_err(|_| ImageError::Encode("first_line overflow".into()))?,
        );
        write_str_id(&mut out, strings.intern_str(&init.function)?);
        write_str_id(&mut out, strings.intern_str(&init.namespace)?);
        write_opt_str(&mut out, init.scope.as_deref(), strings)?;
        match &init.imports {
            Some(imports) => {
                write_u8(&mut out, 1);
                let mut entries: Vec<(u8, &String, &String)> = imports
                    .iter()
                    .map(|((kind, k), v)| (encode_refl_name_kind(*kind), k, v))
                    .collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
                write_len(&mut out, entries.len())?;
                for (kind, key, val) in entries {
                    write_u8(&mut out, kind);
                    write_str_id(&mut out, strings.intern_str(key)?);
                    write_str_id(&mut out, strings.intern_str(val)?);
                }
            }
            None => write_u8(&mut out, 0),
        }
        write_u8(&mut out, u8::from(init.strict_types));
    }

    match &unit.reflection_lines {
        Some(lines) => {
            write_u8(&mut out, 1);
            encode_line_table(&mut out, lines)?;
        }
        None => write_u8(&mut out, 0),
    }

    match &unit.reflection_source {
        Some(source) => {
            write_u8(&mut out, 1);
            write_len(&mut out, source.len())?;
            out.extend_from_slice(source);
        }
        None => write_u8(&mut out, 0),
    }

    // Per-unit facts.
    match &unit.facts {
        Some(facts) => {
            write_u8(&mut out, 1);
            encode_unit_facts(&mut out, facts)?;
        }
        None => write_u8(&mut out, 0),
    }

    Ok(out)
}

fn encode_function(
    out: &mut Vec<u8>,
    func: &Function,
    strings: &mut StringPoolBuilder,
) -> Result<(), ImageError> {
    write_str_id(out, strings.intern_str(&func.name)?);
    write_u32(out, func.definition_line);
    write_u16(out, func.registers);
    write_u16(out, func.captures);
    write_u16(out, func.iterators);
    let flags = u8::from(func.returns_reference)
        | (u8::from(func.is_generator) << 1)
        | (u8::from(func.dynamic_scope) << 2)
        | (u8::from(func.observes_arguments) << 3)
        | (u8::from(func.strict_types) << 4)
        | (u8::from(func.argument_layout == ArgumentLayout::Variadic) << 5);
    write_u8(out, flags);
    write_u32(out, func.return_type.map_or(u32::MAX, |t| t.0));

    write_len(out, func.instructions.len())?;
    for &inst in func.instructions.iter() {
        out.extend_from_slice(&encode_instruction(inst));
    }

    write_len(out, func.lines.len())?;
    for &line in &func.lines {
        write_u32(out, line);
    }

    write_len(out, func.locals.len())?;
    for id in &func.locals {
        write_u32(out, id.0);
    }

    write_len(out, func.parameters.len())?;
    for param in &func.parameters {
        encode_parameter(out, param, strings)?;
    }

    write_len(out, func.paths.len())?;
    for path in &func.paths {
        encode_path(out, path)?;
    }

    write_len(out, func.statics.len())?;
    for id in &func.statics {
        write_u32(out, id.0);
    }

    write_len(out, func.handlers.len())?;
    for handler in &func.handlers {
        encode_handler(out, handler)?;
    }

    write_len(out, func.method_sites.len())?;
    for site in &func.method_sites {
        write_u16(out, site.receiver.0);
        write_u32(out, site.method.0);
        write_u32(out, site.first_argument);
        write_u16(out, site.arguments);
        write_u32(out, site.after.0);
        write_u8(out, u8::from(site.receiver_taken));
    }

    write_len(out, func.site_arguments.len())?;
    for &arg in &func.site_arguments {
        match arg {
            SiteArgument::Local(r) => {
                write_u8(out, 0);
                write_u16(out, r.0);
            }
            SiteArgument::Value(r) => {
                write_u8(out, 1);
                write_u16(out, r.0);
            }
        }
    }

    Ok(())
}

fn encode_parameter(
    out: &mut Vec<u8>,
    param: &Parameter,
    strings: &mut StringPoolBuilder,
) -> Result<(), ImageError> {
    write_opt_str(out, param.default_display.as_deref(), strings)?;
    write_u32(out, param.name.0);
    write_u32(out, param.ty.map_or(u32::MAX, |t| t.0));
    let flags = u8::from(param.by_reference)
        | (u8::from(param.variadic) << 1)
        | (u8::from(param.has_default) << 2);
    write_u8(out, flags);
    Ok(())
}

fn encode_path(out: &mut Vec<u8>, path: &BcPath) -> Result<(), ImageError> {
    match path.base {
        PathBase::This { scratch } => {
            write_u8(out, 0);
            write_u16(out, scratch.0);
        }
        PathBase::StaticProperty {
            class,
            name,
            scratch,
        } => {
            write_u8(out, 1);
            write_u16(out, class.0);
            write_u16(out, name.0);
            write_u16(out, scratch.0);
        }
        PathBase::NamedStaticProperty {
            class,
            name,
            scratch,
        } => {
            write_u8(out, 2);
            write_u32(out, class.0);
            write_u32(out, name.0);
            write_u16(out, scratch.0);
        }
        PathBase::Register(r) => {
            write_u8(out, 3);
            write_u16(out, r.0);
        }
        PathBase::Named(r) => {
            write_u8(out, 4);
            write_u16(out, r.0);
        }
        PathBase::GlobalNamed { name, scratch } => {
            write_u8(out, 5);
            write_u16(out, name.0);
            write_u16(out, scratch.0);
        }
        PathBase::Globals => write_u8(out, 6),
    }
    write_len(out, path.steps.len())?;
    for &step in &path.steps {
        match step {
            Step::Property(n) => {
                write_u8(out, 0);
                write_u32(out, n.0);
            }
            Step::DynamicProperty(r) => {
                write_u8(out, 1);
                write_u16(out, r.0);
            }
            Step::Index(r) => {
                write_u8(out, 2);
                write_u16(out, r.0);
            }
            Step::Append => write_u8(out, 3),
        }
    }
    Ok(())
}

fn encode_handler(out: &mut Vec<u8>, handler: &Handler) -> Result<(), ImageError> {
    write_u32(out, handler.start.0);
    write_u32(out, handler.end.0);
    write_u32(out, handler.catch.map_or(u32::MAX, |t| t.0));
    write_u32(out, handler.finally.map_or(u32::MAX, |t| t.0));
    write_u16(out, handler.exception.0);
    write_len(out, handler.cleanup.len())?;
    for r in &handler.cleanup {
        write_u16(out, r.0);
    }
    Ok(())
}

fn encode_symbol(
    out: &mut Vec<u8>,
    sym: &Symbol,
    strings: &mut StringPoolBuilder,
) -> Result<(), ImageError> {
    write_str_id(out, strings.intern_str(&sym.name)?);
    write_opt_str(out, sym.fallback.as_deref(), strings)?;
    write_str_id(out, strings.intern_str(&sym.written)?);
    Ok(())
}

fn encode_type(
    out: &mut Vec<u8>,
    ty: &Type,
    strings: &mut StringPoolBuilder,
) -> Result<(), ImageError> {
    write_u16(out, ty.mask.0);
    write_u8(out, u8::from(ty.intersection));
    write_str_id(out, strings.intern_str(&ty.display)?);
    write_len(out, ty.classes.len())?;
    for name in &ty.classes {
        write_u32(out, name.0);
    }
    Ok(())
}

fn encode_constant(
    out: &mut Vec<u8>,
    constant: &Constant,
    strings: &mut StringPoolBuilder,
) -> Result<(), ImageError> {
    match constant {
        Constant::Null => write_u8(out, 0),
        Constant::Bool(v) => {
            write_u8(out, 1);
            write_u8(out, u8::from(*v));
        }
        Constant::Int(v) => {
            write_u8(out, 2);
            write_i64(out, *v);
        }
        Constant::Float(v) => {
            write_u8(out, 3);
            write_u64(out, v.to_bits());
        }
        Constant::String(bytes) => {
            write_u8(out, 4);
            write_str_id(out, strings.intern_bytes(bytes)?);
        }
        Constant::Array(items) => {
            write_u8(out, 5);
            write_len(out, items.len())?;
            for (key, value) in items {
                match key {
                    ConstantKey::Next => write_u8(out, 0),
                    ConstantKey::Int(i) => {
                        write_u8(out, 1);
                        write_i64(out, *i);
                    }
                    ConstantKey::String(bytes) => {
                        write_u8(out, 2);
                        write_str_id(out, strings.intern_bytes(bytes)?);
                    }
                }
                encode_constant(out, value, strings)?;
            }
        }
    }
    Ok(())
}

fn encode_class(
    out: &mut Vec<u8>,
    class: &ClassDeclaration,
    strings: &mut StringPoolBuilder,
) -> Result<(), ImageError> {
    encode_decl_meta(out, &class.metadata, strings)?;
    write_u32(out, class.line);
    write_str_id(out, strings.intern_str(&class.name)?);
    write_u32(out, class.parent.map_or(u32::MAX, |s| s.0));
    write_len(out, class.interfaces.len())?;
    for iface in &class.interfaces {
        write_u32(out, iface.0);
    }
    write_u8(out, encode_class_kind(class.kind));
    let flags = u8::from(class.is_abstract)
        | (u8::from(class.is_final) << 1)
        | (u8::from(class.is_readonly) << 2)
        | (u8::from(class.allows_dynamic_properties) << 3);
    write_u8(out, flags);
    write_u8(
        out,
        match class.enum_backing {
            None => 0,
            Some(EnumBacking::Int) => 1,
            Some(EnumBacking::String) => 2,
        },
    );

    write_len(out, class.constants.len())?;
    for c in &class.constants {
        encode_class_constant(out, c, strings)?;
    }

    write_len(out, class.properties.len())?;
    for p in &class.properties {
        encode_property(out, p, strings)?;
    }

    write_len(out, class.methods.len())?;
    for m in &class.methods {
        encode_method(out, m);
    }

    write_len(out, class.traits.len())?;
    for tr in &class.traits {
        encode_trait_use(out, tr, strings)?;
    }

    write_len(out, class.enum_cases.len())?;
    for case in &class.enum_cases {
        encode_enum_case(out, case, strings)?;
    }
    Ok(())
}

fn encode_class_constant(
    out: &mut Vec<u8>,
    c: &ClassConstant,
    strings: &mut StringPoolBuilder,
) -> Result<(), ImageError> {
    encode_decl_meta(out, &c.metadata, strings)?;
    write_u32(out, c.name.0);
    write_u8(out, encode_visibility(c.visibility));
    write_u8(out, u8::from(c.is_final));
    write_u32(out, c.initializer.0);
    write_u32(out, c.ty.map_or(u32::MAX, |t| t.0));
    Ok(())
}

fn encode_property(
    out: &mut Vec<u8>,
    p: &PropertyDeclaration,
    strings: &mut StringPoolBuilder,
) -> Result<(), ImageError> {
    encode_decl_meta(out, &p.metadata, strings)?;
    write_u32(out, p.name.0);
    write_u8(out, encode_visibility(p.visibility));
    write_u8(out, p.set_visibility.map_or(u8::MAX, encode_visibility));
    let flags = u8::from(p.is_final)
        | (u8::from(p.promoted) << 1)
        | (u8::from(p.is_static) << 2)
        | (u8::from(p.is_readonly) << 3);
    write_u8(out, flags);
    write_u32(out, p.ty.map_or(u32::MAX, |t| t.0));
    match p.default {
        None => write_u8(out, 0),
        Some(Initializer::Literal(c)) => {
            write_u8(out, 1);
            write_u32(out, c.0);
        }
        Some(Initializer::Function(f)) => {
            write_u8(out, 2);
            write_u32(out, f.0);
        }
    }
    match &p.hooks {
        None => write_u8(out, 0),
        Some(hooks) => {
            write_u8(out, 1);
            encode_property_hooks(out, hooks);
        }
    }
    Ok(())
}

fn encode_property_hooks(out: &mut Vec<u8>, hooks: &PropertyHooks<phera_bytecode::FunctionId>) {
    write_u8(out, u8::from(hooks.is_virtual));
    encode_opt_hook(out, hooks.get.as_ref());
    encode_opt_hook(out, hooks.set.as_ref());
}

fn encode_opt_hook(out: &mut Vec<u8>, hook: Option<&PropertyHook<phera_bytecode::FunctionId>>) {
    match hook {
        None => write_u8(out, 0),
        Some(h) => {
            write_u8(out, 1);
            write_u32(out, h.function.0);
            write_u8(out, u8::from(h.is_abstract) | (u8::from(h.is_final) << 1));
        }
    }
}

fn encode_method(out: &mut Vec<u8>, m: &MethodDeclaration) {
    write_u32(out, m.name.0);
    write_u32(out, m.function.0);
    write_u8(out, encode_visibility(m.visibility));
    let flags =
        u8::from(m.is_static) | (u8::from(m.is_abstract) << 1) | (u8::from(m.is_final) << 2);
    write_u8(out, flags);
}

fn encode_trait_use(
    out: &mut Vec<u8>,
    tr: &TraitUse,
    strings: &mut StringPoolBuilder,
) -> Result<(), ImageError> {
    write_str_vec(out, &tr.names, strings)?;
    write_len(out, tr.adaptations.len())?;
    for adapt in &tr.adaptations {
        match adapt {
            TraitAdaptation::Precedence {
                trait_name,
                method,
                instead_of,
            } => {
                write_u8(out, 0);
                write_str_id(out, strings.intern_str(trait_name)?);
                write_str_id(out, strings.intern_str(method)?);
                write_str_vec(out, instead_of, strings)?;
            }
            TraitAdaptation::Alias {
                trait_name,
                method,
                alias,
                visibility,
                is_final,
            } => {
                write_u8(out, 1);
                write_opt_str(out, trait_name.as_deref(), strings)?;
                write_str_id(out, strings.intern_str(method)?);
                write_opt_str(out, alias.as_deref(), strings)?;
                write_u8(out, visibility.map_or(u8::MAX, encode_visibility));
                write_u8(out, u8::from(*is_final));
            }
        }
    }
    Ok(())
}

fn encode_enum_case(
    out: &mut Vec<u8>,
    case: &EnumCase,
    strings: &mut StringPoolBuilder,
) -> Result<(), ImageError> {
    encode_decl_meta(out, &case.metadata, strings)?;
    write_u32(out, case.name.0);
    write_u32(out, case.backing.map_or(u32::MAX, |f| f.0));
    Ok(())
}

fn encode_decl_meta(
    out: &mut Vec<u8>,
    meta: &reflection::DeclarationMetadata,
    strings: &mut StringPoolBuilder,
) -> Result<(), ImageError> {
    encode_span(out, meta.span)?;
    encode_span(out, meta.doc_comment)?;
    write_len(out, meta.attributes.len())?;
    for attr in &meta.attributes {
        encode_attr(out, attr, strings)?;
    }
    Ok(())
}

fn encode_attr(
    out: &mut Vec<u8>,
    attr: &reflection::Attribute,
    strings: &mut StringPoolBuilder,
) -> Result<(), ImageError> {
    write_str_id(out, strings.intern_str(&attr.name)?);
    write_u32(
        out,
        u32::try_from(attr.line).map_err(|_| ImageError::Encode("attr line overflow".into()))?,
    );
    write_u8(out, u8::from(attr.strict_types));
    write_opt_str(out, attr.validation_error.as_deref(), strings)?;
    write_len(out, attr.arguments.len())?;
    for arg in &attr.arguments {
        write_opt_str(out, arg.name.as_deref(), strings)?;
        encode_refl_init(out, &arg.initializer, strings)?;
    }
    Ok(())
}

fn encode_refl_init(
    out: &mut Vec<u8>,
    init: &reflection::Initializer,
    strings: &mut StringPoolBuilder,
) -> Result<(), ImageError> {
    match init {
        reflection::Initializer::Null => write_u8(out, 0),
        reflection::Initializer::Bool(v) => {
            write_u8(out, 1);
            write_u8(out, u8::from(*v));
        }
        reflection::Initializer::Int(v) => {
            write_u8(out, 2);
            write_i64(out, *v);
        }
        reflection::Initializer::Float(bits) => {
            write_u8(out, 3);
            write_u64(out, *bits);
        }
        reflection::Initializer::String(bytes) => {
            write_u8(out, 4);
            write_str_id(out, strings.intern_bytes(bytes)?);
        }
        reflection::Initializer::EmptyArray => write_u8(out, 5),
        reflection::Initializer::Expression(id) => {
            write_u8(out, 6);
            write_u32(
                out,
                u32::try_from(id.0)
                    .map_err(|_| ImageError::Encode("InitializerId overflow".into()))?,
            );
        }
    }
    Ok(())
}

fn encode_compile_diagnostic(
    out: &mut Vec<u8>,
    diag: &CompileDiagnostic,
    strings: &mut StringPoolBuilder,
) -> Result<(), ImageError> {
    write_u8(
        out,
        match diag.kind {
            CompileDiagnosticKind::Warning => 0,
            CompileDiagnosticKind::Deprecated => 1,
        },
    );
    write_str_id(out, strings.intern_str(&diag.message)?);
    write_u32(
        out,
        u32::try_from(diag.line).map_err(|_| ImageError::Encode("diag line overflow".into()))?,
    );
    Ok(())
}

fn encode_line_table(out: &mut Vec<u8>, table: &LineTable) -> Result<(), ImageError> {
    write_u32(
        out,
        u32::try_from(table.first_line())
            .map_err(|_| ImageError::Encode("LineTable first_line overflow".into()))?,
    );
    write_len(out, table.starts().len())?;
    for &start in table.starts() {
        write_u64(out, start as u64);
    }
    Ok(())
}

fn encode_span(out: &mut Vec<u8>, span: Span) -> Result<(), ImageError> {
    write_u64(out, span.start as u64);
    write_u64(out, span.end as u64);
    Ok(())
}

fn encode_program_facts(
    facts: &ProgramFacts,
    strings: &mut StringPoolBuilder,
) -> Result<Vec<u8>, ImageError> {
    let mut out = Vec::new();
    write_u8(
        &mut out,
        match facts.mode {
            BuildMode::Hybrid => 0,
            BuildMode::Strict => 1,
        },
    );
    write_len(&mut out, facts.units.len())?;
    for unit_path in facts.units.iter() {
        write_str_id(&mut out, strings.intern_str(unit_path)?);
    }
    write_len(&mut out, facts.classes.len())?;
    for layout in facts.classes.iter() {
        encode_class_layout(&mut out, layout, strings)?;
    }
    write_len(&mut out, facts.descendants.len())?;
    for desc in facts.descendants.iter() {
        write_u32(&mut out, desc.0);
    }
    write_len(&mut out, facts.functions.len())?;
    for (name, decl, func_ref) in facts.functions.iter() {
        write_str_id(&mut out, strings.intern_str(name)?);
        encode_decl_uniqueness(&mut out, *decl);
        write_u32(&mut out, func_ref.unit.0);
        write_u32(&mut out, func_ref.function);
    }
    Ok(out)
}

fn encode_class_layout(
    out: &mut Vec<u8>,
    layout: &AotClassLayout,
    strings: &mut StringPoolBuilder,
) -> Result<(), ImageError> {
    write_str_id(out, strings.intern_str(&layout.name)?);
    encode_decl_uniqueness(out, layout.decl);
    write_u8(out, encode_class_kind(layout.kind));
    write_u32(out, layout.parent.map_or(u32::MAX, |c| c.0));
    write_len(out, layout.ancestors.len())?;
    for anc in layout.ancestors.iter() {
        write_u32(out, anc.0);
    }
    write_u32(out, layout.first_descendant);
    write_u32(out, layout.descendants);
    write_u16(out, layout.flags.0);
    write_len(out, layout.slots.len())?;
    for slot in layout.slots.iter() {
        encode_prop_slot(out, slot, strings)?;
    }
    write_len(out, layout.methods.len())?;
    for method in layout.methods.iter() {
        encode_aot_method(out, method, strings)?;
    }
    Ok(())
}

fn encode_prop_slot(
    out: &mut Vec<u8>,
    slot: &AotPropSlot,
    strings: &mut StringPoolBuilder,
) -> Result<(), ImageError> {
    write_str_id(out, strings.intern_bytes(&slot.key)?);
    write_u32(out, slot.declaring.0);
    write_u8(out, slot.flags.0);
    encode_sound_type(out, slot.ty);
    Ok(())
}

fn encode_aot_method(
    out: &mut Vec<u8>,
    method: &AotMethod,
    strings: &mut StringPoolBuilder,
) -> Result<(), ImageError> {
    write_str_id(out, strings.intern_str(&method.name)?);
    write_u32(out, method.function.unit.0);
    write_u32(out, method.function.function);
    write_u8(out, method.flags.0);
    Ok(())
}

fn encode_unit_facts(out: &mut Vec<u8>, facts: &UnitBuildFacts) -> Result<(), ImageError> {
    write_u32(out, facts.unit.0);
    write_len(out, facts.classes.len())?;
    for c in facts.classes.iter() {
        write_u32(out, c.0);
    }
    write_len(out, facts.functions.len())?;
    for func in facts.functions.iter() {
        encode_function_facts(out, func)?;
    }
    Ok(())
}

fn encode_function_facts(out: &mut Vec<u8>, facts: &FunctionBuildFacts) -> Result<(), ImageError> {
    write_u8(out, facts.flags.0);
    write_len(out, facts.locals.len())?;
    for local in facts.locals.iter() {
        write_u8(out, local.0);
    }
    write_len(out, facts.block_starts.len())?;
    for &start in facts.block_starts.iter() {
        write_u32(out, start);
    }
    write_len(out, facts.block_types.len())?;
    for &(block, reg, ty) in facts.block_types.iter() {
        write_u16(out, block);
        write_u16(out, reg.0);
        encode_sound_type(out, ty);
    }
    write_len(out, facts.calls.len())?;
    for call in facts.calls.iter() {
        encode_resolved_call(out, *call);
    }
    write_len(out, facts.props.len())?;
    for prop in facts.props.iter() {
        encode_resolved_prop(out, *prop);
    }
    encode_sound_type(out, facts.returns);
    Ok(())
}

fn encode_resolved_call(out: &mut Vec<u8>, call: ResolvedCall) {
    write_u32(out, call.at);
    write_u32(out, call.target.unit.0);
    write_u32(out, call.target.function);
    write_u8(out, call.needs.0);
    write_u8(out, call.flags.0);
    encode_sound_type(out, call.result);
}

fn encode_resolved_prop(out: &mut Vec<u8>, prop: ResolvedProp) {
    write_u32(out, prop.at);
    write_u32(out, prop.class.0);
    write_u16(out, prop.slot);
    write_u8(out, prop.needs.0);
    write_u8(out, prop.flags.0);
    encode_sound_type(out, prop.ty);
}

fn encode_sound_type(out: &mut Vec<u8>, ty: SoundType) {
    write_u16(out, ty.mask.0);
    match ty.class {
        ClassBound::Any => {
            write_u8(out, 0);
            write_u32(out, 0);
        }
        ClassBound::Exact(c) => {
            write_u8(out, 1);
            write_u32(out, c.0);
        }
        ClassBound::Within(c) => {
            write_u8(out, 2);
            write_u32(out, c.0);
        }
    }
}

fn encode_decl_uniqueness(out: &mut Vec<u8>, decl: DeclUniqueness) {
    match decl {
        DeclUniqueness::Unique { unit, hoisted } => {
            write_u8(out, 0);
            write_u32(out, unit.0);
            write_u8(out, u8::from(hoisted));
        }
        DeclUniqueness::Duplicate => write_u8(out, 1),
        DeclUniqueness::Native => write_u8(out, 2),
        DeclUniqueness::Unknown => write_u8(out, 3),
    }
}

fn encode_audit(
    audit: &DynamicAudit,
    strings: &mut StringPoolBuilder,
) -> Result<Vec<u8>, ImageError> {
    let mut out = Vec::new();
    write_u8(&mut out, audit.holds.0);
    write_len(&mut out, audit.findings.len())?;
    for finding in audit.findings.iter() {
        encode_finding(&mut out, finding, strings)?;
    }
    Ok(out)
}

fn encode_finding(
    out: &mut Vec<u8>,
    finding: &DynamicFinding,
    strings: &mut StringPoolBuilder,
) -> Result<(), ImageError> {
    write_u8(out, encode_dynamic_kind(finding.kind));
    write_u8(
        out,
        match finding.radius {
            phera_bytecode::facts::BlastRadius::Function => 0,
            phera_bytecode::facts::BlastRadius::Class => 1,
            phera_bytecode::facts::BlastRadius::Names => 2,
        },
    );
    write_u32(out, finding.at.unit.0);
    write_u32(out, finding.at.function);
    write_u32(out, finding.line);
    write_str_id(out, strings.intern_str(&finding.detail)?);
    Ok(())
}

fn encode_dynamic_kind(kind: DynamicKind) -> u8 {
    match kind {
        DynamicKind::Eval => 0,
        DynamicKind::DynamicInclude => 1,
        DynamicKind::DynamicNew => 2,
        DynamicKind::DynamicStaticCall => 3,
        DynamicKind::DynamicMethodCall => 4,
        DynamicKind::ClassAlias => 5,
        DynamicKind::Extract => 6,
        DynamicKind::Compact => 7,
        DynamicKind::Unserialize => 8,
        DynamicKind::Reflection => 9,
        DynamicKind::VariableVariable => 10,
    }
}

fn encode_class_kind(kind: ClassKind) -> u8 {
    match kind {
        ClassKind::Class => 0,
        ClassKind::Interface => 1,
        ClassKind::Anonymous => 2,
        ClassKind::Trait => 3,
        ClassKind::Enum => 4,
    }
}

fn encode_visibility(vis: Visibility) -> u8 {
    match vis {
        Visibility::Public => 0,
        Visibility::Protected => 1,
        Visibility::Private => 2,
    }
}

fn encode_refl_name_kind(kind: reflection::NameKind) -> u8 {
    match kind {
        reflection::NameKind::Class => 0,
        reflection::NameKind::Function => 1,
        reflection::NameKind::Constant => 2,
    }
}

fn write_u8(out: &mut Vec<u8>, v: u8) {
    out.push(v);
}

fn write_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn write_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn write_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn write_i64(out: &mut Vec<u8>, v: i64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn write_str_id(out: &mut Vec<u8>, id: StringId) {
    write_u32(out, id.0);
}

fn write_len(out: &mut Vec<u8>, len: usize) -> Result<(), ImageError> {
    let len =
        u32::try_from(len).map_err(|_| ImageError::Encode("collection len > u32::MAX".into()))?;
    write_u32(out, len);
    Ok(())
}

fn write_opt_str(
    out: &mut Vec<u8>,
    value: Option<&str>,
    strings: &mut StringPoolBuilder,
) -> Result<(), ImageError> {
    match value {
        Some(s) => write_str_id(out, strings.intern_str(s)?),
        None => write_u32(out, u32::MAX),
    }
    Ok(())
}

fn write_str_vec(
    out: &mut Vec<u8>,
    values: &[String],
    strings: &mut StringPoolBuilder,
) -> Result<(), ImageError> {
    write_len(out, values.len())?;
    for v in values {
        write_str_id(out, strings.intern_str(v)?);
    }
    Ok(())
}

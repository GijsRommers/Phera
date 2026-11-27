//! Linear post-decode validator for [`phera_bytecode::CompiledUnit`].
//!
//! Runs once when a unit blob is decoded from a `.phb` image, guaranteeing that
//! every register, jump target, table index, handler range, path, method site,
//! class member, and reflection reference is in bounds before the VM executes it.

use phera_bytecode::{
    ClassId, CompiledUnit, ConstId, Function, FunctionId, Initializer, Instruction, IterId,
    MethodSiteId, NameId, PathBase, PathId, Reg, StaticId, Step, SymbolId, Target, TypeId,
    reflection,
};

use crate::error::ImageError;

/// Validates all structural invariants and index bounds of `unit`.
pub fn validate_unit(unit: &CompiledUnit) -> Result<(), ImageError> {
    let names = unit.names.len();
    let symbols = unit.symbols.len();
    let types = unit.types.len();
    let constants = unit.constants.len();
    let functions = unit.functions.len();
    let classes = unit.classes.len();
    let refl_inits = unit.reflection_initializers.len();

    for (idx, ty) in unit.types.iter().enumerate() {
        for &name in &ty.classes {
            check_name(name, names, "type class", idx)?;
        }
    }

    for (idx, &fn_id) in unit.hoisted.iter().enumerate() {
        check_function(fn_id, functions, "hoisted function", idx)?;
    }

    for (idx, &class_id) in unit.hoisted_classes.iter().enumerate() {
        check_class(class_id, classes, "hoisted class", idx)?;
    }

    if !unit.function_metadata.is_empty() && unit.function_metadata.len() != functions {
        return Err(ImageError::Validation(format!(
            "function_metadata len {} != functions len {functions}",
            unit.function_metadata.len()
        )));
    }
    for meta in &unit.function_metadata {
        validate_decl_meta(&meta.declaration, refl_inits)?;
        for param in &meta.parameters {
            for attr in &param.attributes {
                validate_attr(attr, refl_inits)?;
            }
            if let Some(init) = &param.default {
                validate_refl_init(init, refl_inits)?;
            }
        }
    }
    for meta in unit.constant_metadata.values() {
        validate_decl_meta(meta, refl_inits)?;
    }

    for (idx, class) in unit.classes.iter().enumerate() {
        validate_decl_meta(&class.metadata, refl_inits)?;
        if let Some(parent) = class.parent {
            check_symbol(parent, symbols, "class parent", idx)?;
        }
        for &iface in &class.interfaces {
            check_symbol(iface, symbols, "class interface", idx)?;
        }
        for c in &class.constants {
            validate_decl_meta(&c.metadata, refl_inits)?;
            check_name(c.name, names, "class constant name", idx)?;
            check_function(c.initializer, functions, "class constant initializer", idx)?;
            if let Some(ty) = c.ty {
                check_type(ty, types, "class constant type", idx)?;
            }
        }
        for prop in &class.properties {
            validate_decl_meta(&prop.metadata, refl_inits)?;
            check_name(prop.name, names, "property name", idx)?;
            if let Some(ty) = prop.ty {
                check_type(ty, types, "property type", idx)?;
            }
            if let Some(default) = prop.default {
                match default {
                    Initializer::Literal(c) => check_const(c, constants, "property default", idx)?,
                    Initializer::Function(f) => {
                        check_function(f, functions, "property default fn", idx)?
                    }
                }
            }
            if let Some(hooks) = &prop.hooks {
                if let Some(get) = &hooks.get {
                    check_function(get.function, functions, "property get hook", idx)?;
                }
                if let Some(set) = &hooks.set {
                    check_function(set.function, functions, "property set hook", idx)?;
                }
            }
        }
        for method in &class.methods {
            check_name(method.name, names, "method name", idx)?;
            check_function(method.function, functions, "method body", idx)?;
        }
        for case in &class.enum_cases {
            validate_decl_meta(&case.metadata, refl_inits)?;
            check_name(case.name, names, "enum case name", idx)?;
            if let Some(backing) = case.backing {
                check_function(backing, functions, "enum case backing", idx)?;
            }
        }
    }

    validate_function(&unit.main, unit, "{main}")?;
    for (idx, func) in unit.functions.iter().enumerate() {
        validate_function(func, unit, &format!("function#{idx} ({})", func.name))?;
    }

    if let Some(facts) = &unit.facts {
        let expected_fns = 1 + functions;
        if facts.functions.len() != expected_fns {
            return Err(ImageError::Validation(format!(
                "unit facts function count {} != {expected_fns}",
                facts.functions.len()
            )));
        }
        if facts.classes.len() != classes {
            return Err(ImageError::Validation(format!(
                "unit facts class count {} != {classes}",
                facts.classes.len()
            )));
        }
        for (idx, (func_facts, func)) in facts
            .functions
            .iter()
            .zip(std::iter::once(&unit.main).chain(&unit.functions))
            .enumerate()
        {
            if func_facts.locals.len() > func.locals.len() {
                return Err(ImageError::Validation(format!(
                    "unit facts function#{idx} locals {} > function locals {}",
                    func_facts.locals.len(),
                    func.locals.len()
                )));
            }
            let ins_len = func.instructions.len();
            let regs = func.registers as usize;
            for (bi, &start) in func_facts.block_starts.iter().enumerate() {
                if (start as usize) >= ins_len {
                    return Err(ImageError::Validation(format!(
                        "unit facts function#{idx} block#{bi} start {start} >= {ins_len}"
                    )));
                }
                if bi > 0 && start <= func_facts.block_starts[bi - 1] {
                    return Err(ImageError::Validation(format!(
                        "unit facts function#{idx} block_starts not strictly ascending at #{bi}"
                    )));
                }
            }
            for &(block, reg, _) in &func_facts.block_types {
                if (block as usize) >= func_facts.block_starts.len() {
                    return Err(ImageError::Validation(format!(
                        "unit facts function#{idx} block_type block {block} >= {}",
                        func_facts.block_starts.len()
                    )));
                }
                if reg.index() >= regs {
                    return Err(ImageError::Validation(format!(
                        "unit facts function#{idx} block_type reg r{} >= {regs}",
                        reg.0
                    )));
                }
            }
            for call in &func_facts.calls {
                if (call.at as usize) >= ins_len {
                    return Err(ImageError::Validation(format!(
                        "unit facts function#{idx} call at {} >= {ins_len}",
                        call.at
                    )));
                }
            }
            for prop in &func_facts.props {
                if (prop.at as usize) >= ins_len {
                    return Err(ImageError::Validation(format!(
                        "unit facts function#{idx} prop at {} >= {ins_len}",
                        prop.at
                    )));
                }
            }
        }
    }

    Ok(())
}

fn validate_function(func: &Function, unit: &CompiledUnit, label: &str) -> Result<(), ImageError> {
    let regs = func.registers as usize;
    let ins_len = func.instructions.len();
    let names = unit.names.len();
    let symbols = unit.symbols.len();
    let types = unit.types.len();
    let constants = unit.constants.len();
    let functions = unit.functions.len();
    let classes = unit.classes.len();
    let paths = func.paths.len();
    let statics = func.statics.len();
    let iters = func.iterators as usize;
    let sites = func.method_sites.len();

    if func.lines.len() != ins_len {
        return Err(ImageError::Validation(format!(
            "{label}: lines len {} != instructions len {ins_len}",
            func.lines.len()
        )));
    }
    if func.locals.len() > regs {
        return Err(ImageError::Validation(format!(
            "{label}: locals len {} > registers {regs}",
            func.locals.len()
        )));
    }
    let param_and_capture_slots = func.parameters.len().saturating_add(func.captures as usize);
    if param_and_capture_slots > func.locals.len() {
        return Err(ImageError::Validation(format!(
            "{label}: parameters ({}) + captures ({}) > locals ({})",
            func.parameters.len(),
            func.captures,
            func.locals.len()
        )));
    }
    for (i, &name) in func.locals.iter().enumerate() {
        check_name(name, names, label, i)?;
    }
    for (i, param) in func.parameters.iter().enumerate() {
        check_name(param.name, names, label, i)?;
        if let Some(ty) = param.ty {
            check_type(ty, types, label, i)?;
        }
    }
    if let Some(ty) = func.return_type {
        check_type(ty, types, label, 0)?;
    }
    for (i, &name) in func.statics.iter().enumerate() {
        check_name(name, names, label, i)?;
    }

    for (i, handler) in func.handlers.iter().enumerate() {
        check_target_inclusive(handler.start, ins_len, label, i)?;
        check_target_inclusive(handler.end, ins_len, label, i)?;
        if handler.start.0 > handler.end.0 {
            return Err(ImageError::Validation(format!(
                "{label}: handler#{i} start {} > end {}",
                handler.start.0, handler.end.0
            )));
        }
        if let Some(t) = handler.catch {
            check_target(t, ins_len, label, i)?;
        }
        if let Some(t) = handler.finally {
            check_target(t, ins_len, label, i)?;
        }
        check_reg(handler.exception, regs, label, i)?;
        for &reg in &handler.cleanup {
            check_reg(reg, regs, label, i)?;
        }
    }

    for (i, path) in func.paths.iter().enumerate() {
        match path.base {
            PathBase::This { scratch } => check_reg(scratch, regs, label, i)?,
            PathBase::StaticProperty {
                class,
                name,
                scratch,
            } => {
                check_reg(class, regs, label, i)?;
                check_reg(name, regs, label, i)?;
                check_reg(scratch, regs, label, i)?;
            }
            PathBase::NamedStaticProperty {
                class,
                name,
                scratch,
            } => {
                check_symbol(class, symbols, label, i)?;
                check_name(name, names, label, i)?;
                check_reg(scratch, regs, label, i)?;
            }
            PathBase::Register(r) | PathBase::Named(r) => check_reg(r, regs, label, i)?,
            PathBase::GlobalNamed { name, scratch } => {
                check_reg(name, regs, label, i)?;
                check_reg(scratch, regs, label, i)?;
            }
            PathBase::Globals => {}
        }
        for &step in &path.steps {
            match step {
                Step::Property(n) => check_name(n, names, label, i)?,
                Step::DynamicProperty(r) | Step::Index(r) => check_reg(r, regs, label, i)?,
                Step::Append => {}
            }
        }
    }

    for (i, site) in func.method_sites.iter().enumerate() {
        check_reg(site.receiver, regs, label, i)?;
        check_name(site.method, names, label, i)?;
        check_target(site.after, ins_len, label, i)?;
        let end = (site.first_argument as usize)
            .checked_add(site.arguments as usize)
            .ok_or_else(|| ImageError::Validation(format!("{label}: method site#{i} overflow")))?;
        if end > func.site_arguments.len() {
            return Err(ImageError::Validation(format!(
                "{label}: method site#{i} argument range {end} > {}",
                func.site_arguments.len()
            )));
        }
    }
    for (i, arg) in func.site_arguments.iter().enumerate() {
        check_reg(arg.register(), regs, label, i)?;
    }

    for (ip, &inst) in func.instructions.iter().enumerate() {
        validate_instruction(
            inst,
            &FnBounds {
                label,
                ip,
                regs,
                ins_len,
                names,
                symbols,
                types,
                constants,
                functions,
                classes,
                paths,
                statics,
                iters,
                sites,
            },
        )?;
    }

    Ok(())
}

struct FnBounds<'a> {
    label: &'a str,
    ip: usize,
    regs: usize,
    ins_len: usize,
    names: usize,
    symbols: usize,
    types: usize,
    constants: usize,
    functions: usize,
    classes: usize,
    paths: usize,
    statics: usize,
    iters: usize,
    sites: usize,
}

fn validate_instruction(inst: Instruction, b: &FnBounds<'_>) -> Result<(), ImageError> {
    let reg = |r| check_reg(r, b.regs, b.label, b.ip);
    let tgt = |t| check_target(t, b.ins_len, b.label, b.ip);
    let name = |n| check_name(n, b.names, b.label, b.ip);
    let sym = |s| check_symbol(s, b.symbols, b.label, b.ip);
    let ty = |t| check_type(t, b.types, b.label, b.ip);
    let cst = |c| check_const(c, b.constants, b.label, b.ip);
    let func = |f| check_function(f, b.functions, b.label, b.ip);
    let cls = |c| check_class(c, b.classes, b.label, b.ip);
    let path = |p| check_path(p, b.paths, b.label, b.ip);
    let st = |s| check_static(s, b.statics, b.label, b.ip);
    let iter = |i| check_iter(i, b.iters, b.label, b.ip);
    let site = |s| check_site(s, b.sites, b.label, b.ip);

    match inst {
        Instruction::Nop
        | Instruction::GeneratorStart
        | Instruction::AttributeDiagnostics
        | Instruction::ReturnNull
        | Instruction::ReturnMissing
        | Instruction::BeginSilence
        | Instruction::EndSilence => Ok(()),

        Instruction::Yield { dst, key, value } => {
            reg(dst)?;
            if let Some(k) = key {
                reg(k)?;
            }
            if let Some(v) = value {
                reg(v)?;
            }
            Ok(())
        }
        Instruction::YieldFrom { dst, src }
        | Instruction::Move { dst, src }
        | Instruction::Assign { local: dst, src }
        | Instruction::AssignTake { local: dst, src }
        | Instruction::AssignOp {
            local: dst, src, ..
        }
        | Instruction::IncDecLocal {
            dst, local: src, ..
        }
        | Instruction::IssetLocal { dst, local: src }
        | Instruction::AppendLocal { local: dst, src }
        | Instruction::Unary { dst, src, .. }
        | Instruction::Cast { dst, src, .. }
        | Instruction::ArrayPush { array: dst, src }
        | Instruction::ArraySpread { array: dst, src }
        | Instruction::ArrayPushReference { array: dst, src }
        | Instruction::Include { dst, path: src, .. }
        | Instruction::DynamicMethodCall {
            receiver: dst,
            method: src,
        }
        | Instruction::StaticCall {
            class: dst,
            method: src,
            ..
        }
        | Instruction::Eval { dst, source: src }
        | Instruction::BuiltinUnary { dst, src, .. }
        | Instruction::NamespacedBuiltin { dst, src, .. }
        | Instruction::BinaryInt { dst, lhs: src, .. } => {
            reg(dst)?;
            reg(src)
        }

        Instruction::GeneratorReturn { src }
        | Instruction::CheckReturn { source: src }
        | Instruction::ReturnChecked { source: src }
        | Instruction::FinallyReturn { state: src }
        | Instruction::FinallyDiscard { state: src }
        | Instruction::Throw { source: src }
        | Instruction::FetchThis { dst: src }
        | Instruction::LoadNull { dst: src }
        | Instruction::LoadBool { dst: src, .. }
        | Instruction::LoadInt { dst: src, .. }
        | Instruction::Clear { reg: src }
        | Instruction::UnsetLocal { local: src }
        | Instruction::BindDynamicGlobal { name: src }
        | Instruction::NewArray { dst: src, .. }
        | Instruction::InitDynamicCall { callee: src }
        | Instruction::SendValue { src, .. }
        | Instruction::SendLocal { local: src }
        | Instruction::SendSpread { src }
        | Instruction::Call { dst: src, .. }
        | Instruction::SendTake { src, .. }
        | Instruction::MakeCallable { dst: src }
        | Instruction::ReturnReference { src }
        | Instruction::Return { src }
        | Instruction::Echo { src }
        | Instruction::Exit { src }
        | Instruction::Raise { detail: src, .. }
        | Instruction::NewDynamic { class: src }
        | Instruction::ThisOrNull { dst: src }
        | Instruction::Capture { src, .. }
        | Instruction::BuiltinFrame { dst: src, .. }
        | Instruction::InitUserCallback { callback: src, .. }
        | Instruction::SendUserCallback { src, .. } => reg(src),

        Instruction::Binary { dst, lhs, rhs, .. }
        | Instruction::ArraySet {
            array: dst,
            key: lhs,
            src: rhs,
        }
        | Instruction::ArraySetReference {
            array: dst,
            key: lhs,
            src: rhs,
        }
        | Instruction::DynamicInstanceOf {
            dst,
            source: lhs,
            class: rhs,
        }
        | Instruction::FetchDynamicClassConstant {
            dst,
            class: lhs,
            name: rhs,
        }
        | Instruction::ArrayKeyExists {
            dst,
            key: lhs,
            array: rhs,
        } => {
            reg(dst)?;
            reg(lhs)?;
            reg(rhs)
        }

        Instruction::Clone { dst, src, with } => {
            reg(dst)?;
            reg(src)?;
            if let Some(w) = with {
                reg(w)?;
            }
            Ok(())
        }

        Instruction::FinallyCall { state, target, .. }
        | Instruction::BinaryIntBranch {
            lhs: state, target, ..
        }
        | Instruction::BinaryIntBranchIf {
            lhs: state, target, ..
        }
        | Instruction::JumpIf {
            condition: state,
            target,
        }
        | Instruction::JumpIfNot {
            condition: state,
            target,
        }
        | Instruction::JumpIfNull {
            value: state,
            target,
        }
        | Instruction::JumpIfNotNull {
            value: state,
            target,
        }
        | Instruction::ReceiveDefault {
            param: state,
            passed: target,
        } => {
            reg(state)?;
            tgt(target)
        }

        Instruction::BinaryBranch {
            lhs, rhs, target, ..
        }
        | Instruction::BinaryBranchIf {
            lhs, rhs, target, ..
        }
        | Instruction::DynamicInstanceOfBranch {
            src: lhs,
            class: rhs,
            target,
        }
        | Instruction::DynamicInstanceOfBranchIf {
            src: lhs,
            class: rhs,
            target,
        } => {
            reg(lhs)?;
            reg(rhs)?;
            tgt(target)
        }

        Instruction::Jump { target } => tgt(target),

        Instruction::FetchClassConstant {
            dst,
            class,
            name: n,
        } => {
            reg(dst)?;
            reg(class)?;
            name(n)
        }
        Instruction::FetchNamedClassConstant {
            dst,
            class,
            name: n,
        } => {
            reg(dst)?;
            sym(class)?;
            name(n)
        }
        Instruction::DeclareClass { class } => cls(class),
        Instruction::New { class } | Instruction::InitCall { function: class } => sym(class),
        Instruction::InitMethodCall { receiver, method } => {
            reg(receiver)?;
            name(method)
        }
        Instruction::FetchClassName { dst, class }
        | Instruction::FetchConstant {
            dst,
            constant: class,
        } => {
            reg(dst)?;
            sym(class)
        }
        Instruction::InstanceOf { dst, src, class } => {
            reg(dst)?;
            reg(src)?;
            sym(class)
        }
        Instruction::InstanceOfBranch { src, class, target }
        | Instruction::InstanceOfBranchIf { src, class, target } => {
            reg(src)?;
            sym(class)?;
            tgt(target)
        }
        Instruction::LoadConst { dst, constant }
        | Instruction::AssignConst {
            local: dst,
            constant,
        } => {
            reg(dst)?;
            cst(constant)
        }
        Instruction::SendConst { constant } => cst(constant),
        Instruction::InLiteralArray {
            dst,
            needle,
            haystack,
            ..
        } => {
            reg(dst)?;
            reg(needle)?;
            cst(haystack)
        }
        Instruction::Fetch { dst, path: p, .. }
        | Instruction::AssignPath { path: p, src: dst }
        | Instruction::IncDecPath { dst, path: p, .. }
        | Instruction::IssetPath { dst, path: p }
        | Instruction::MakeReference { dst, path: p }
        | Instruction::BindReference { path: p, src: dst } => {
            reg(dst)?;
            path(p)
        }
        Instruction::AssignPathFromLocal { path: p, src, dst }
        | Instruction::AssignOpPath {
            dst, path: p, src, ..
        } => {
            reg(src)?;
            reg(dst)?;
            path(p)
        }
        Instruction::UnsetPath { path: p } | Instruction::SendPath { path: p } => path(p),
        Instruction::FetchIndex {
            dst,
            base,
            index,
            path: p,
            ..
        } => {
            reg(dst)?;
            reg(base)?;
            reg(index)?;
            path(p)
        }
        Instruction::BindGlobal { local, name: n }
        | Instruction::DeclareConstant {
            name: n,
            src: local,
        } => {
            reg(local)?;
            name(n)
        }
        Instruction::NameArgument { name: n } => name(n),
        Instruction::BindStatic {
            local,
            variable,
            initialized,
        } => {
            reg(local)?;
            st(variable)?;
            tgt(initialized)
        }
        Instruction::InitStatic {
            local,
            variable,
            src,
        } => {
            reg(local)?;
            st(variable)?;
            reg(src)
        }
        Instruction::IterStart { iter: i, src, end }
        | Instruction::IterNext {
            iter: i,
            value: src,
            end,
        } => {
            iter(i)?;
            reg(src)?;
            tgt(end)
        }
        Instruction::IterStartReference {
            subject,
            iter: i,
            path: p,
            end,
        } => {
            reg(subject)?;
            iter(i)?;
            path(p)?;
            tgt(end)
        }
        Instruction::IterNextKeyed {
            iter: i,
            key,
            value,
            end,
        } => {
            iter(i)?;
            reg(key)?;
            reg(value)?;
            tgt(end)
        }
        Instruction::IterEnd { iter: i } => iter(i),
        Instruction::CallMethod { site: s, dst, .. } => {
            site(s)?;
            reg(dst)
        }
        Instruction::CallDirect {
            function,
            args,
            count,
            dst,
            ..
        } => {
            func(function)?;
            reg(dst)?;
            if count > 0 {
                let end = args.index().checked_add(count as usize).ok_or_else(|| {
                    ImageError::Validation(format!("{}: CallDirect arg range overflow", b.label))
                })?;
                if end > b.regs {
                    return Err(ImageError::Validation(format!(
                        "{}@{}: CallDirect arg range {}..{end} > registers {}",
                        b.label, b.ip, args.0, b.regs
                    )));
                }
            }
            Ok(())
        }
        Instruction::CallNativeDirect {
            function,
            args,
            count,
            dst,
            ..
        } => {
            sym(function)?;
            reg(dst)?;
            if count > 0 {
                let end = args.index().checked_add(count as usize).ok_or_else(|| {
                    ImageError::Validation(format!(
                        "{}: CallNativeDirect arg range overflow",
                        b.label
                    ))
                })?;
                if end > b.regs {
                    return Err(ImageError::Validation(format!(
                        "{}@{}: CallNativeDirect arg range {}..{end} > registers {}",
                        b.label, b.ip, args.0, b.regs
                    )));
                }
            }
            Ok(())
        }
        Instruction::CheckType { local, ty: t } => {
            reg(local)?;
            ty(t)
        }
        Instruction::DeclareFunction { function } => func(function),
        Instruction::InitStaticCall { class, method, .. } => {
            sym(class)?;
            sym(method)
        }
        Instruction::MakeClosure { dst, function, .. } => {
            reg(dst)?;
            func(function)
        }
        Instruction::GuardBuiltin { function, call } => {
            sym(function)?;
            tgt(call)
        }
    }
}

fn validate_decl_meta(
    meta: &reflection::DeclarationMetadata,
    refl_inits: usize,
) -> Result<(), ImageError> {
    for attr in &meta.attributes {
        validate_attr(attr, refl_inits)?;
    }
    Ok(())
}

fn validate_attr(attr: &reflection::Attribute, refl_inits: usize) -> Result<(), ImageError> {
    for arg in &attr.arguments {
        validate_refl_init(&arg.initializer, refl_inits)?;
    }
    Ok(())
}

fn validate_refl_init(init: &reflection::Initializer, refl_inits: usize) -> Result<(), ImageError> {
    if let reflection::Initializer::Expression(id) = init
        && id.0 >= refl_inits
    {
        return Err(ImageError::Validation(format!(
            "reflection initializer id {} >= {refl_inits}",
            id.0
        )));
    }
    Ok(())
}

fn check_reg(r: Reg, max: usize, label: &str, at: usize) -> Result<(), ImageError> {
    if r.index() >= max {
        return Err(ImageError::Validation(format!(
            "{label}@{at}: register r{} >= {max}",
            r.0
        )));
    }
    Ok(())
}

fn check_target(t: Target, max: usize, label: &str, at: usize) -> Result<(), ImageError> {
    if t.index() >= max {
        return Err(ImageError::Validation(format!(
            "{label}@{at}: target @{} >= {max}",
            t.0
        )));
    }
    Ok(())
}

fn check_target_inclusive(t: Target, max: usize, label: &str, at: usize) -> Result<(), ImageError> {
    if t.index() > max {
        return Err(ImageError::Validation(format!(
            "{label}@{at}: range target @{} > {max}",
            t.0
        )));
    }
    Ok(())
}

fn check_name(n: NameId, max: usize, label: &str, at: usize) -> Result<(), ImageError> {
    if n.index() >= max {
        return Err(ImageError::Validation(format!(
            "{label}@{at}: name#{} >= {max}",
            n.0
        )));
    }
    Ok(())
}

fn check_symbol(s: SymbolId, max: usize, label: &str, at: usize) -> Result<(), ImageError> {
    if s.index() >= max {
        return Err(ImageError::Validation(format!(
            "{label}@{at}: symbol#{} >= {max}",
            s.0
        )));
    }
    Ok(())
}

fn check_type(t: TypeId, max: usize, label: &str, at: usize) -> Result<(), ImageError> {
    if t.index() >= max {
        return Err(ImageError::Validation(format!(
            "{label}@{at}: type#{} >= {max}",
            t.0
        )));
    }
    Ok(())
}

fn check_const(c: ConstId, max: usize, label: &str, at: usize) -> Result<(), ImageError> {
    if c.index() >= max {
        return Err(ImageError::Validation(format!(
            "{label}@{at}: const#{} >= {max}",
            c.0
        )));
    }
    Ok(())
}

fn check_function(f: FunctionId, max: usize, label: &str, at: usize) -> Result<(), ImageError> {
    if f.index() >= max {
        return Err(ImageError::Validation(format!(
            "{label}@{at}: function#{} >= {max}",
            f.0
        )));
    }
    Ok(())
}

fn check_class(c: ClassId, max: usize, label: &str, at: usize) -> Result<(), ImageError> {
    if c.index() >= max {
        return Err(ImageError::Validation(format!(
            "{label}@{at}: class#{} >= {max}",
            c.0
        )));
    }
    Ok(())
}

fn check_path(p: PathId, max: usize, label: &str, at: usize) -> Result<(), ImageError> {
    if p.index() >= max {
        return Err(ImageError::Validation(format!(
            "{label}@{at}: path#{} >= {max}",
            p.0
        )));
    }
    Ok(())
}

fn check_static(s: StaticId, max: usize, label: &str, at: usize) -> Result<(), ImageError> {
    if s.index() >= max {
        return Err(ImageError::Validation(format!(
            "{label}@{at}: static#{} >= {max}",
            s.0
        )));
    }
    Ok(())
}

fn check_iter(i: IterId, max: usize, label: &str, at: usize) -> Result<(), ImageError> {
    if i.index() >= max {
        return Err(ImageError::Validation(format!(
            "{label}@{at}: iter#{} >= {max}",
            i.0
        )));
    }
    Ok(())
}

fn check_site(s: MethodSiteId, max: usize, label: &str, at: usize) -> Result<(), ImageError> {
    if s.index() >= max {
        return Err(ImageError::Validation(format!(
            "{label}@{at}: site#{} >= {max}",
            s.0
        )));
    }
    Ok(())
}

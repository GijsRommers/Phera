//! SplFileInfo keeps its lexical path and asks streams for file metadata.
//!
//! ```php
//! $info = new SplFileInfo('src/report.csv');
//! echo $info->getExtension(); // csv
//! ```

use phera_ext_streams::native;
use phera_extension_api::{
    ArrayKey, ClassEntry, ClassHooks, ConstantValue, Context, MethodEntry as Method, NativeClone,
    NativeError, NativeResult, Object, Param, ParamType, PhpArray, PropertyPurpose, Registry,
    Value, args,
};

use super::{Data, FILE, INFO, Location, State, clone_state, read, update};
use crate::{This, receiver};

pub(super) const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Copy(clone_state)),
    properties: Some(view),
    ..ClassHooks::EMPTY
};

const NAME: Param = Param::new("filename", ParamType::String);
pub(super) const SUFFIX: Param =
    Param::new("suffix", ParamType::String).with_default(ConstantValue::String(""));
const CLASS: Param = Param::new("class", ParamType::String);
const OPTIONAL_CLASS: Param = CLASS.nullable().with_default(ConstantValue::Null);

pub(super) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        interfaces: &["Stringable"],
        hooks: Some(&HOOKS),
        methods: const {
            &[
                Method::public("__construct", &[NAME], construct),
                Method::public("getPath", &[], get_path).tentative_return("string"),
                Method::public("getFilename", &[], filename).tentative_return("string"),
                Method::public("getExtension", &[], extension).tentative_return("string"),
                Method::public("getBasename", &[SUFFIX], basename).tentative_return("string"),
                Method::public("getPathname", &[], pathname).tentative_return("string"),
                Method::public_async("getPerms", &[], perms).tentative_return("int|false"),
                Method::public_async("getInode", &[], inode).tentative_return("int|false"),
                Method::public_async("getSize", &[], size).tentative_return("int|false"),
                Method::public_async("getOwner", &[], owner).tentative_return("int|false"),
                Method::public_async("getGroup", &[], group).tentative_return("int|false"),
                Method::public_async("getATime", &[], atime).tentative_return("int|false"),
                Method::public_async("getMTime", &[], mtime).tentative_return("int|false"),
                Method::public_async("getCTime", &[], ctime).tentative_return("int|false"),
                Method::public_async("getType", &[], file_type).tentative_return("string|false"),
                Method::public_async("isWritable", &[], writable).tentative_return("bool"),
                Method::public_async("isReadable", &[], readable).tentative_return("bool"),
                Method::public_async("isExecutable", &[], executable).tentative_return("bool"),
                Method::public_async("isFile", &[], is_file).tentative_return("bool"),
                Method::public_async("isDir", &[], is_dir).tentative_return("bool"),
                Method::public_async("isLink", &[], is_link).tentative_return("bool"),
                Method::public("getLinkTarget", &[], link_target).tentative_return("string|false"),
                Method::public("getRealPath", &[], real_path).tentative_return("string|false"),
                Method::public_async("openFile", super::file::OPEN_PARAMS, super::file::open_file)
                    .tentative_return(FILE),
                Method::public_async("getFileInfo", &[OPTIONAL_CLASS], file_info)
                    .with_preparation(prepare_class_name)
                    .tentative_return(INFO),
                Method::public_async("getPathInfo", &[OPTIONAL_CLASS], path_info)
                    .with_preparation(prepare_class_name)
                    .tentative_return("?SplFileInfo"),
                Method::public_async(
                    "setInfoClass",
                    const { &[CLASS.with_default(ConstantValue::String(INFO))] },
                    set_info,
                )
                .with_preparation(prepare_class_name)
                .tentative_return("void"),
                Method::public_async(
                    "setFileClass",
                    const { &[CLASS.with_default(ConstantValue::String(FILE))] },
                    set_file,
                )
                .with_preparation(prepare_class_name)
                .tentative_return("void"),
                Method::public("__toString", &[], pathname).returning("string"),
                Method::public("__debugInfo", &[], debug).tentative_return("array"),
            ]
        },
        ..ClassEntry::class(INFO)
    });
}

fn construct(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let name = args::bytes(arguments, 0);
    native::check_path("SplFileInfo::__construct", "#1 ($filename)", name)?;
    update(receiver(this)?, |state| {
        state.data = Data::Info(Location::info(name))
    })?;
    Ok(Value::Null)
}

fn get_path(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    read(receiver(this)?, |state| {
        state
            .location()
            .map_or(Value::from(""), |location| Value::String(location.path))
    })
}

fn pathname(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    read(receiver(this)?, |state| {
        if let Data::Directory(directory) = &state.data
            && directory.entry().is_none()
        {
            return Value::from("");
        }
        state
            .location()
            .map_or(Value::from(""), |location| Value::String(location.pathname))
    })
}

pub(super) fn location(object: &Object) -> Result<Location, NativeError> {
    read(object, State::location)?
}

fn filename(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    Ok(Value::from(location(receiver(this)?)?.filename().to_vec()))
}

pub(super) fn extension(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let location = location(receiver(this)?)?;
    let base = native::basename(location.filename());
    let suffix = base
        .iter()
        .rposition(|byte| *byte == b'.')
        .map_or(&[][..], |dot| &base[dot + 1..]);
    Ok(Value::from(suffix.to_vec()))
}

pub(super) fn basename(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let location = location(receiver(this)?)?;
    let base = native::basename(location.filename());
    let suffix = args::bytes(arguments, 0);
    let base = if !suffix.is_empty() && suffix.len() < base.len() {
        base.strip_suffix(suffix).unwrap_or(base)
    } else {
        base
    };
    Ok(Value::from(base.to_vec()))
}

fn real_path(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let path = read(object, |state| match &state.data {
        Data::Uninitialized => None,
        Data::Directory(directory) if directory.entry().is_none() => None,
        _ => state.location().ok().map(|location| location.pathname),
    })?;
    Ok(path.map_or(Value::Bool(false), |path| {
        native::canonical_path(context, path.as_bytes())
    }))
}

fn link_target(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let pathname = location(receiver(this)?)?.pathname;
    if pathname.as_bytes().is_empty() {
        context.warning("SplFileInfo::getLinkTarget(): No such file or directory".into());
        return Ok(Value::Bool(false));
    }
    native::read_link(context, pathname.as_bytes())
        .map(Value::from)
        .map_err(|error| {
            NativeError::throw(
                "RuntimeException",
                format!(
                    "Unable to read link {}, error: {}",
                    String::from_utf8_lossy(pathname.as_bytes()),
                    native::reason(&error)
                ),
            )
        })
}

pub(super) fn view(
    _: &mut dyn Context,
    object: &Object,
    purpose: PropertyPurpose,
) -> Result<PhpArray, NativeError> {
    if purpose != PropertyPurpose::Debug {
        return Ok(object.to_array());
    }
    read(object, |state| {
        let mut props = object.to_array();
        let location = state.location().ok();
        let visible = !matches!(&state.data, Data::Directory(dir) if dir.entry().is_none());
        let current = location.as_ref().filter(|_| visible);
        private(
            &mut props,
            INFO,
            "pathName",
            current.map_or(Value::from(""), |path| Value::String(path.pathname.clone())),
        );
        if let Some(path) = current {
            private(
                &mut props,
                INFO,
                "fileName",
                Value::from(path.filename().to_vec()),
            );
        }
        match &state.data {
            Data::Directory(directory) => directory.debug(&mut props),
            Data::File(file) => file.debug(&mut props),
            _ => (),
        }
        props
    })
}

pub(super) fn private(props: &mut PhpArray, class: &str, name: &str, value: Value) {
    props.insert(ArrayKey::str(format!("\0{class}\0{name}")), value);
}

fn debug(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    view(context, receiver(this)?, PropertyPurpose::Debug).map(Value::from)
}

mod factories;
mod status;
pub(super) use factories::create_info;
use factories::{file_info, path_info, prepare_class_name, set_file, set_info};
use status::*;

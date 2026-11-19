//! DirectoryIterator, FilesystemIterator, RecursiveDirectoryIterator and GlobIterator.
//!
//! ```php
//! $php = new GlobIterator('src/*.php');
//! echo count($php);
//! ```

use phera_extension_api::{
    ClassEntry, ClassHooks, ConstantValue, MethodEntry as Method, NativeClone, Param, ParamType,
    Registry,
};

use super::{DIRECTORY, FILESYSTEM, GLOB, INFO, RECURSIVE, directories as d, information as i};

const PATH: Param = Param::new("directory", ParamType::String);
const PATTERN: Param = Param::new("pattern", ParamType::String);
const FLAGS: Param = Param::new("flags", ParamType::Int);
const OFFSET: Param = Param::new("offset", ParamType::Int);
const ALLOW_LINKS: Param =
    Param::new("allowLinks", ParamType::Bool).with_default(ConstantValue::Bool(false));

const CONSTANTS: &[(&str, ConstantValue)] = &[
    ("CURRENT_MODE_MASK", ConstantValue::Int(d::CURRENT_MASK)),
    ("CURRENT_AS_PATHNAME", ConstantValue::Int(d::AS_PATHNAME)),
    ("CURRENT_AS_FILEINFO", ConstantValue::Int(0)),
    ("CURRENT_AS_SELF", ConstantValue::Int(d::AS_SELF)),
    ("KEY_MODE_MASK", ConstantValue::Int(d::KEY_MASK)),
    ("KEY_AS_PATHNAME", ConstantValue::Int(0)),
    ("FOLLOW_SYMLINKS", ConstantValue::Int(d::FOLLOW_LINKS)),
    ("KEY_AS_FILENAME", ConstantValue::Int(d::KEY_FILENAME)),
    ("NEW_CURRENT_AND_KEY", ConstantValue::Int(d::KEY_FILENAME)),
    ("OTHER_MODE_MASK", ConstantValue::Int(d::OTHER_MASK)),
    ("SKIP_DOTS", ConstantValue::Int(d::SKIP_DOTS)),
    ("UNIX_PATHS", ConstantValue::Int(8192)),
];

const DIRECTORY_HOOKS: ClassHooks = ClassHooks {
    iterator: Some(d::create),
    ..ClassHooks::EMPTY
};

const GLOB_HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Forbidden),
    count: Some(d::native_count),
    ..ClassHooks::EMPTY
};

pub(super) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        parent: Some(INFO),
        interfaces: &["SeekableIterator"],
        hooks: Some(&DIRECTORY_HOOKS),
        methods: const {
            &[
                Method::public_async("__construct", &[PATH], d::construct),
                Method::public("getFilename", &[], d::filename).tentative_return("string"),
                Method::public("getExtension", &[], i::extension).tentative_return("string"),
                Method::public("getBasename", &[i::SUFFIX], i::basename).tentative_return("string"),
                Method::public("isDot", &[], d::is_dot).tentative_return("bool"),
                Method::public("rewind", &[], d::rewind).tentative_return("void"),
                Method::public("valid", &[], d::valid).tentative_return("bool"),
                Method::public("key", &[], d::index).tentative_return("mixed"),
                Method::public("current", &[], d::identity).tentative_return("mixed"),
                Method::public("next", &[], d::next).tentative_return("void"),
                Method::public_async("seek", &[OFFSET], d::seek).tentative_return("void"),
                Method::public("__toString", &[], d::filename).returning("string"),
            ]
        },
        ..ClassEntry::class(DIRECTORY)
    });
    registry.class(ClassEntry {
        parent: Some(DIRECTORY),
        constants: CONSTANTS,
        methods: const {
            &[
                Method::public_async(
                    "__construct",
                    const { &[PATH, FLAGS.with_default(ConstantValue::Int(d::SKIP_DOTS))] },
                    d::filesystem,
                ),
                Method::public("rewind", &[], d::rewind).tentative_return("void"),
                Method::public("key", &[], d::key).tentative_return("string"),
                Method::public_async("current", &[], d::current)
                    .tentative_return("string|SplFileInfo|FilesystemIterator"),
                Method::public("getFlags", &[], d::flags).tentative_return("int"),
                Method::public("setFlags", &[FLAGS], d::set_flags).tentative_return("void"),
            ]
        },
        ..ClassEntry::class(FILESYSTEM)
    });
    registry.class(ClassEntry {
        parent: Some(FILESYSTEM),
        interfaces: &["RecursiveIterator"],
        methods: const {
            &[
                Method::public_async(
                    "__construct",
                    const { &[PATH, FLAGS.with_default(ConstantValue::Int(0))] },
                    d::recursive,
                ),
                Method::public("hasChildren", &[ALLOW_LINKS], d::has_children)
                    .tentative_return("bool"),
                Method::public_async("getChildren", &[], d::children).tentative_return(RECURSIVE),
                Method::public("getSubPath", &[], d::sub_path).tentative_return("string"),
                Method::public("getSubPathname", &[], d::sub_pathname).tentative_return("string"),
            ]
        },
        ..ClassEntry::class(RECURSIVE)
    });
    registry.class(ClassEntry {
        parent: Some(FILESYSTEM),
        interfaces: &["Countable"],
        hooks: Some(&GLOB_HOOKS),
        methods: const {
            &[
                Method::public_async(
                    "__construct",
                    const { &[PATTERN, FLAGS.with_default(ConstantValue::Int(0))] },
                    d::glob,
                ),
                Method::public("count", &[], d::count).tentative_return("int"),
            ]
        },
        ..ClassEntry::class(GLOB)
    });
}

//! File and temporary file objects expose PHP's stream and line protocols.
//!
//! ```php
//! $file = new SplTempFileObject(4096);
//! $file->fwrite("row\n");
//! ```

use phera_extension_api::{
    ClassEntry, ClassHooks, ConstantValue, MethodEntry as Method, NativeClone, Param, ParamType,
    Registry,
};

use super::{
    DROP_NEW_LINE, FILE, READ_AHEAD, READ_CSV, SKIP_EMPTY, TEMP, construct, construct_temp, csv,
    lines, operations as op,
};
use crate::files::INFO;

const NAME: Param = Param::new("filename", ParamType::String);
const MODE: Param = Param::new("mode", ParamType::String).with_default(ConstantValue::String("r"));
const USE_PATH: Param =
    Param::new("useIncludePath", ParamType::Bool).with_default(ConstantValue::Bool(false));
const CONTEXT: Param = Param::new("context", ParamType::Mixed).optional();
pub(in crate::files) const OPEN_PARAMS: &[Param] = &[MODE, USE_PATH, CONTEXT];
const CONSTRUCT_PARAMS: &[Param] = &[NAME, MODE, USE_PATH, CONTEXT];
const MAX_MEMORY: Param = Param::new("maxMemory", ParamType::Int).optional();
const SEPARATOR: Param =
    Param::new("separator", ParamType::String).with_default(ConstantValue::String(","));
const ENCLOSURE: Param =
    Param::new("enclosure", ParamType::String).with_default(ConstantValue::String("\""));
const ESCAPE: Param =
    Param::new("escape", ParamType::String).with_default(ConstantValue::String("\\"));
const CSV: &[Param] = &[SEPARATOR, ENCLOSURE, ESCAPE];
const WRITE_CSV: &[Param] = &[
    Param::new("fields", ParamType::Array),
    SEPARATOR,
    ENCLOSURE,
    ESCAPE,
    Param::new("eol", ParamType::String).with_default(ConstantValue::String("\n")),
];
const LOCK: &[Param] = &[
    Param::new("operation", ParamType::Int),
    Param::new("wouldBlock", ParamType::Mixed)
        .by_ref()
        .optional(),
];
const SEEK: &[Param] = &[
    Param::new("offset", ParamType::Int),
    Param::new("whence", ParamType::Int).with_default(ConstantValue::Int(0)),
];
const SCAN: &[Param] = &[
    Param::new("format", ParamType::String),
    Param::new("vars", ParamType::Mixed).by_ref().variadic(),
];
const LENGTH: Param = Param::new("length", ParamType::Int);
const WRITE: &[Param] = &[
    Param::new("data", ParamType::String),
    LENGTH.nullable().with_default(ConstantValue::Null),
];

const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Forbidden),
    ..ClassHooks::EMPTY
};

pub(in crate::files) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        parent: Some(INFO),
        interfaces: &["RecursiveIterator", "SeekableIterator"],
        hooks: Some(&HOOKS),
        constants: &[
            ("DROP_NEW_LINE", ConstantValue::Int(DROP_NEW_LINE)),
            ("READ_AHEAD", ConstantValue::Int(READ_AHEAD)),
            ("SKIP_EMPTY", ConstantValue::Int(SKIP_EMPTY)),
            ("READ_CSV", ConstantValue::Int(READ_CSV)),
        ],
        methods: const {
            &[
                Method::public_async("__construct", CONSTRUCT_PARAMS, construct),
                Method::public_async("rewind", &[], lines::rewind).tentative_return("void"),
                Method::public("eof", &[], lines::eof).tentative_return("bool"),
                Method::public("valid", &[], lines::valid).tentative_return("bool"),
                Method::public_async("fgets", &[], lines::fgets).tentative_return("string"),
                Method::public_async("fread", &[LENGTH], op::read).tentative_return("string|false"),
                Method::public_async("fgetcsv", CSV, csv::get_csv).tentative_return("array|false"),
                Method::public_async("fputcsv", WRITE_CSV, csv::put_csv)
                    .tentative_return("int|false"),
                Method::public("setCsvControl", CSV, csv::set).tentative_return("void"),
                Method::public("getCsvControl", &[], csv::read_controls).tentative_return("array"),
                Method::public_async("flock", LOCK, op::lock).tentative_return("bool"),
                Method::public_async("fflush", &[], op::flush).tentative_return("bool"),
                Method::public_async("ftell", &[], op::tell).tentative_return("int|false"),
                Method::public_async("fseek", SEEK, op::seek).tentative_return("int"),
                Method::public_async("fgetc", &[], op::character).tentative_return("string|false"),
                Method::public_async("fpassthru", &[], op::passthru).tentative_return("int"),
                Method::public_async("fscanf", SCAN, op::scan).tentative_return("array|int|null"),
                Method::public_async("fwrite", WRITE, op::write).tentative_return("int|false"),
                Method::public_async("fstat", &[], op::stat).tentative_return("array"),
                Method::public_async(
                    "ftruncate",
                    const { &[Param::new("size", ParamType::Int)] },
                    op::truncate,
                )
                .tentative_return("bool"),
                Method::public_async("current", &[], lines::current)
                    .tentative_return("string|array|false"),
                Method::public("key", &[], lines::key).tentative_return("int"),
                Method::public_async("next", &[], lines::next).tentative_return("void"),
                Method::public(
                    "setFlags",
                    const { &[Param::new("flags", ParamType::Int)] },
                    lines::set_flags,
                )
                .tentative_return("void"),
                Method::public("getFlags", &[], lines::flags).tentative_return("int"),
                Method::public(
                    "setMaxLineLen",
                    const { &[Param::new("maxLength", ParamType::Int)] },
                    lines::set_max_line,
                )
                .tentative_return("void"),
                Method::public("getMaxLineLen", &[], lines::max_line).tentative_return("int"),
                Method::public("hasChildren", &[], op::has_children).tentative_return("false"),
                Method::public("getChildren", &[], op::children).tentative_return("null"),
                Method::public_async(
                    "seek",
                    const { &[Param::new("line", ParamType::Int)] },
                    lines::seek,
                )
                .tentative_return("void"),
                Method::public_async("getCurrentLine", &[], lines::fgets)
                    .tentative_return("string"),
                Method::public_async("__toString", &[], lines::string).returning("string"),
            ]
        },
        ..ClassEntry::class(FILE)
    });
    registry.class(ClassEntry {
        parent: Some(FILE),
        methods: const {
            &[Method::public_async(
                "__construct",
                &[MAX_MEMORY],
                construct_temp,
            )]
        },
        ..ClassEntry::class(TEMP)
    });
}

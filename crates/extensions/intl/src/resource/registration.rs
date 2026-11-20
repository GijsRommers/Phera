//! Native ResourceBundle signatures and read-only dimensions.

use phera_extension_api::{
    ClassEntry, ClassHooks, ConstantValue, FunctionEntry, MethodEntry, NativeClone,
    NativeDimensions, Param, ParamType, Registry, Value, args,
};

use super::{CLASS, State, count, create, get, last_error, locales, lookup, make_cursor, missing};
use super::{read_only, update};

const LOCALE: Param = Param::new("locale", ParamType::String).nullable();
const NAME: Param = Param::new("bundle", ParamType::String);
const FALLBACK: Param =
    Param::new("fallback", ParamType::Bool).with_default(ConstantValue::Bool(true));
const CREATE: &[Param] = &[LOCALE, NAME.nullable(), FALLBACK];
const OBJECT: Param = Param::new("bundle", ParamType::Class(CLASS));
const INDEX: Param = Param::new("index", ParamType::IntOrString);
const GET: &[Param] = &[INDEX, FALLBACK];
const FN_GET: &[Param] = &[OBJECT, INDEX, FALLBACK];
const LOCALES: &[Param] = &[NAME];
const INSTANCE: &[Param] = &[OBJECT];
const ITEM: &str = "ResourceBundle|array|string|int|null";
const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::copy::<State>()),
    fork: Some(phera_extension_api::fork_clone::<State>),
    iterator: Some(make_cursor),
    count: Some(count),
    dimensions: Some(NativeDimensions {
        get: |cx, object, index| get(cx, object, &index, true, "ResourceBundle::get"),
        exists: |_, object, index| update(object, |state| lookup(&state.table, &index).is_some()),
        set: |_, _, _, _| Err(read_only()),
        unset: |_, _, _| Err(read_only()),
        reference: None,
        bind: None,
        direct: None,
        fallback_if_overridden: &[],
    }),
    ..ClassHooks::EMPTY
};

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        hooks: Some(&HOOKS),
        interfaces: &["IteratorAggregate", "Countable"],
        methods: const {
            &[
                MethodEntry::public_async("__construct", CREATE, |cx, object, values| {
                    create(cx, object, values, "ResourceBundle::__construct")
                }),
                MethodEntry::public_async("create", CREATE, |cx, _, values| {
                    create(cx, None, values, "ResourceBundle::create")
                })
                .static_()
                .returning("?ResourceBundle"),
                MethodEntry::public("get", GET, |cx, object, values| {
                    get(
                        cx,
                        object.ok_or_else(missing)?,
                        args::value(values, 0),
                        args::bool(values, 1).unwrap_or(true),
                        "ResourceBundle::get",
                    )
                })
                .returning(ITEM),
                MethodEntry::public("count", &[], |cx, object, _| {
                    count(cx, object.ok_or_else(missing)?).map(Value::Int)
                })
                .returning("int"),
                MethodEntry::public_async("getLocales", LOCALES, |cx, _, values| {
                    locales(cx, values, "ResourceBundle::getLocales")
                })
                .static_()
                .returning("array|false"),
                MethodEntry::public("getErrorCode", &[], |_, object, _| {
                    last_error(object.ok_or_else(missing)?, false)
                })
                .returning("int"),
                MethodEntry::public("getErrorMessage", &[], |_, object, _| {
                    last_error(object.ok_or_else(missing)?, true)
                })
                .returning("string"),
                MethodEntry::public("getIterator", &[], |cx, object, _| {
                    let cursor = make_cursor(cx, object.ok_or_else(missing)?.clone())?;
                    cx.create_iterator(cursor)
                })
                .returning("Iterator"),
            ]
        },
        ..ClassEntry::class(CLASS)
    });
    for (entry, result) in [
        (
            FunctionEntry::asynchronous("resourcebundle_create", CREATE, |cx, values| {
                create(cx, None, values, "resourcebundle_create")
            }),
            "?ResourceBundle",
        ),
        (
            FunctionEntry::sync("resourcebundle_get", FN_GET, |cx, values| {
                get(
                    cx,
                    args::object(values, 0).ok_or_else(missing)?,
                    args::value(values, 1),
                    args::bool(values, 2).unwrap_or(true),
                    "resourcebundle_get",
                )
            }),
            ITEM,
        ),
        (
            FunctionEntry::sync("resourcebundle_count", INSTANCE, |cx, values| {
                count(cx, args::object(values, 0).ok_or_else(missing)?).map(Value::Int)
            }),
            "int",
        ),
        (
            FunctionEntry::asynchronous("resourcebundle_locales", LOCALES, |cx, values| {
                locales(cx, values, "resourcebundle_locales")
            }),
            "array|false",
        ),
        (
            FunctionEntry::sync("resourcebundle_get_error_code", INSTANCE, |_, values| {
                last_error(args::object(values, 0).ok_or_else(missing)?, false)
            }),
            "int",
        ),
        (
            FunctionEntry::sync("resourcebundle_get_error_message", INSTANCE, |_, values| {
                last_error(args::object(values, 0).ok_or_else(missing)?, true)
            }),
            "string",
        ),
    ] {
        registry.function(entry);
        registry.describe_function(entry.name, result, None);
    }
}

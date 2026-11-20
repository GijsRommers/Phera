//! `stat('virtual://report')` converts the named entries returned by `url_stat()`.

use phera_extension_api::{
    ArrayKey, Context, NativeError, NativeResult, PhpArray, Value, value::to_int,
};
use rustix::{
    fs::Access,
    process::{Gid, getegid, geteuid, getgroups},
};

use crate::metadata::{Query, STAT_NAMES, StatField, stat_array};
use crate::user_file::{invoke, missing};
use crate::user_wrappers::UserWrapper;
use crate::wrapper::Target;

#[derive(Default)]
struct StatusCache {
    followed: Option<(Vec<u8>, [i64; 13])>,
    link: Option<(Vec<u8>, [i64; 13])>,
}

pub(crate) fn clear(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    context.state().take::<StatusCache>();
    Ok(Value::Null)
}

pub(crate) async fn url_stat(
    context: &mut dyn Context,
    wrapper: UserWrapper,
    uri: &[u8],
    function: &str,
    query: Query,
    throws: bool,
) -> NativeResult {
    if !wrapper.permitted(context, function) {
        return query.failed(context, uri, function, throws);
    }
    let follow = query.follow();
    if let Some(cached) = context.state().get::<StatusCache>()
        && let Some((_, fields)) = if follow {
            &cached.followed
        } else {
            &cached.link
        }
        .as_ref()
        .filter(|(path, _)| path == uri)
    {
        return query_value(query, *fields);
    }
    let path = match Target::parse(uri) {
        Target::File(path) => path,
        _ => uri,
    };
    let flags = 4 | if query.quiet() { 2 } else { 0 } | if query.follow() { 0 } else { 1 };
    let object = wrapper.object(context, Value::Null).await?;
    let result = invoke(
        context,
        &object,
        "url_stat",
        vec![Value::from(path.to_vec()), Value::Int(flags)],
    )
    .await?;
    match result {
        Some(Value::Array(array)) => {
            let fields = fields(context, &array)?;
            let cache = context.state().get_or_default::<StatusCache>();
            if !follow {
                cache.link = Some((uri.to_vec(), fields));
            }
            if follow || fields[StatField::Mode as usize] & 0o170000 != 0o120000 {
                cache.followed = Some((uri.to_vec(), fields));
            }
            query_value(query, fields)
        }
        None => {
            missing(context, &object, function, "url_stat", "");
            query.failed(context, uri, function, throws)
        }
        _ => query.failed(context, uri, function, throws),
    }
}

pub(crate) fn fields(
    context: &mut dyn Context,
    input: &PhpArray,
) -> Result<[i64; 13], NativeError> {
    let mut fields = [0; 13];
    for (name, value) in STAT_NAMES.into_iter().zip(&mut fields) {
        if let Some(field) = input.get(&ArrayKey::str(name)) {
            *value = to_int(field, context.diagnostics())?;
        }
    }
    Ok(fields)
}

fn query_value(query: Query, fields: [i64; 13]) -> NativeResult {
    let mode = fields[StatField::Mode as usize];
    let kind = mode & 0o170000;
    Ok(match query {
        Query::Stat | Query::LinkStat => return stat_array(fields),
        Query::Field(field) => Value::Int(fields[field as usize]),
        Query::IsFile => Value::Bool(kind == 0o100000),
        Query::IsDirectory => Value::Bool(kind == 0o040000),
        Query::IsLink => Value::Bool(kind == 0o120000),
        Query::Type => Value::from(match kind {
            0o010000 => "fifo",
            0o020000 => "char",
            0o040000 => "dir",
            0o060000 => "block",
            0o100000 => "file",
            0o120000 => "link",
            0o140000 => "socket",
            _ => "unknown",
        }),
        Query::Access(access) => Value::Bool(permits(access, mode, fields)),
    })
}

fn permits(access: Access, mode: i64, fields: [i64; 13]) -> bool {
    if access == Access::EXISTS {
        return true;
    }
    let uid = geteuid().as_raw();
    if uid == 0 {
        return access != Access::EXEC_OK || mode & 0o111 != 0;
    }
    let gid = fields[StatField::Gid as usize] as u32;
    let shift = if i64::from(uid) == fields[StatField::Uid as usize] {
        6
    } else if getegid().as_raw() == gid
        || getgroups().is_ok_and(|groups| groups.contains(&Gid::from_raw(gid)))
    {
        3
    } else {
        0
    };
    mode & (i64::from(access.bits()) << shift) != 0
}

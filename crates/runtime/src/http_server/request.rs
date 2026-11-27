//! The `Phera\Http\Request` class: a lazy handle on one request's bytes.

use std::borrow::Cow;
use std::cell::OnceCell;
use std::rc::Rc;

use phera_ext_standard::query::{Duplicates, parse_pairs};
use phera_extension_api::{
    ArrayKey, ClassEntry, ClassKind, ConstantValue, Context, MethodEntry, NativeError,
    NativeResult, Object, Param, ParamType, PhpArray, PhpString, Registry, Value, Visibility, args,
};

use super::REQUEST_CLASS;
use super::router::{ParamRanges, RouteId, Router};
use crate::HttpRequest;

/// Native state of a `Phera\Http\Request` object.
pub(super) struct RequestData {
    request: HttpRequest,
    router: Rc<Router>,
    /// `None` when the fallback answers.
    route: Option<RouteId>,
    params: ParamRanges,
    /// Query string parsed on first use.
    query: OnceCell<Rc<PhpArray>>,
}

impl RequestData {
    pub(super) fn new(
        request: HttpRequest,
        router: Rc<Router>,
        route: Option<RouteId>,
        params: ParamRanges,
    ) -> Self {
        RequestData {
            request,
            router,
            route,
            params,
            query: OnceCell::new(),
        }
    }

    fn params(&self) -> impl Iterator<Item = (&str, &[u8])> {
        let names = match self.route {
            Some(route) => self.router.param_names(route),
            None => &[],
        };
        let path = self.request.path.as_bytes();
        names
            .iter()
            .zip(&self.params)
            .map(move |(name, &(start, end))| (&**name, &path[start as usize..end as usize]))
    }

    fn query(&self) -> &Rc<PhpArray> {
        self.query.get_or_init(|| {
            let mut variables = PhpArray::new();
            parse_pairs(
                &mut variables,
                self.request.query.as_bytes(),
                b'&',
                Duplicates::Replace,
            );
            Rc::new(variables)
        })
    }

    fn header(&self, name: &str) -> Option<Cow<'_, [u8]>> {
        let mut values = self
            .request
            .headers
            .iter()
            .filter(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_slice());
        let first = values.next()?;
        let Some(second) = values.next() else {
            return Some(Cow::Borrowed(first));
        };
        let separator: &[u8] = if name.eq_ignore_ascii_case("cookie") {
            b"; "
        } else {
            b", "
        };
        let mut joined = first.to_vec();
        for value in std::iter::once(second).chain(values) {
            joined.extend_from_slice(separator);
            joined.extend_from_slice(value);
        }
        Some(Cow::Owned(joined))
    }
}

pub(super) fn register(registry: &mut Registry) {
    const NAME_WITH_DEFAULT: &[Param] = &[
        Param::new("name", ParamType::String),
        Param::new("default", ParamType::String)
            .nullable()
            .with_default(ConstantValue::Null),
    ];
    const QUERY: &[Param] = &[
        Param::new("name", ParamType::String)
            .nullable()
            .with_default(ConstantValue::Null),
        Param::new("default", ParamType::Mixed).with_default(ConstantValue::Null),
    ];
    const JSON: &[Param] =
        &[Param::new("associative", ParamType::Bool).with_default(ConstantValue::Bool(true))];
    const METHODS: &[MethodEntry] = &[
        MethodEntry::public("__construct", &[], private_construct)
            .with_visibility(Visibility::Private),
        MethodEntry::public("method", &[], method).returning("string"),
        MethodEntry::public("path", &[], path).returning("string"),
        MethodEntry::public("uri", &[], uri).returning("string"),
        MethodEntry::public("param", NAME_WITH_DEFAULT, param).returning("?string"),
        MethodEntry::public("params", &[], params).returning("array"),
        MethodEntry::public("query", QUERY, query).returning("mixed"),
        MethodEntry::public("header", NAME_WITH_DEFAULT, header).returning("?string"),
        MethodEntry::public("headers", &[], headers).returning("array"),
        MethodEntry::public("body", &[], body).returning("string"),
        MethodEntry::public("json", JSON, json).returning("mixed"),
    ];
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        methods: METHODS,
        ..ClassEntry::class(REQUEST_CLASS)
    });
}

pub(super) fn new_object(context: &mut dyn Context, data: RequestData) -> NativeResult {
    let object = context.create_object(REQUEST_CLASS, PhpArray::new())?;
    if let Value::Object(object) = &object {
        *object.native.borrow_mut() = Some(Box::new(data));
    }
    Ok(object)
}

fn with_request<R>(
    this: Option<&Rc<Object>>,
    read: impl FnOnce(&RequestData) -> R,
) -> Result<R, NativeError> {
    let object = this.ok_or_else(|| {
        NativeError::error("Phera\\Http\\Request method called without an object")
    })?;
    let native = object.native.borrow();
    let data = native
        .as_ref()
        .and_then(|native| native.downcast_ref::<RequestData>())
        .ok_or_else(|| NativeError::error("Phera\\Http\\Request object is not initialized"))?;
    Ok(read(data))
}

fn private_construct(_: &mut dyn Context, _: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(Value::Null)
}

fn method(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_request(this, |data| Value::from(data.request.method.as_str()))
}

fn path(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_request(this, |data| Value::from(data.request.path.as_str()))
}

fn uri(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_request(this, |data| Value::from(data.request.target.as_str()))
}

fn default_argument(arguments: &[Value]) -> Value {
    args::value(arguments, 1).clone()
}

fn param(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let name = args::bytes(arguments, 0);
    with_request(this, |data| {
        data.params()
            .find(|(candidate, _)| candidate.as_bytes() == name)
            .map(|(_, raw)| Value::from(percent_decode(raw).as_ref()))
            .unwrap_or_else(|| default_argument(arguments))
    })
}

fn params(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_request(this, |data| {
        let mut params = PhpArray::new();
        for (name, raw) in data.params() {
            params.insert(
                ArrayKey::from_string(PhpString::from(name)),
                Value::from(percent_decode(raw).as_ref()),
            );
        }
        Value::from(params)
    })
}

fn query(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    with_request(this, |data| {
        let query = data.query();
        let Some(name) = args::string(arguments, 0) else {
            return Value::Array(query.clone());
        };
        let key = ArrayKey::from_string(PhpString::from(name.as_bytes()));
        query
            .get(&key)
            .cloned()
            .unwrap_or_else(|| default_argument(arguments))
    })
}

fn header(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let name = String::from_utf8_lossy(args::bytes(arguments, 0));
    with_request(this, |data| {
        data.header(&name)
            .map(|value| Value::from(value.as_ref()))
            .unwrap_or_else(|| default_argument(arguments))
    })
}

fn headers(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_request(this, |data| {
        let mut headers = PhpArray::new();
        for (name, _) in &data.request.headers {
            let key = ArrayKey::from_string(PhpString::from(name.as_str()));
            if headers.get(&key).is_none()
                && let Some(value) = data.header(name)
            {
                headers.insert(key, Value::from(value.as_ref()));
            }
        }
        Value::from(headers)
    })
}

fn body(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_request(this, |data| Value::from(data.request.body.as_slice()))
}

fn json(context: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let associative = args::bool(arguments, 0).unwrap_or(true);
    with_request(this, |data| {
        if data.request.body.is_empty() {
            return Ok(Value::Null);
        }
        phera_ext_json::decode(
            context,
            &data.request.body,
            associative,
            phera_ext_json::flag::THROW_ON_ERROR,
        )
    })?
}

fn percent_decode(raw: &[u8]) -> Cow<'_, [u8]> {
    if !raw.contains(&b'%') {
        return Cow::Borrowed(raw);
    }
    let mut decoded = Vec::with_capacity(raw.len());
    let mut position = 0;
    while position < raw.len() {
        let byte = raw[position];
        if byte == b'%'
            && let Some(high) = raw.get(position + 1).and_then(|&digit| hex_value(digit))
            && let Some(low) = raw.get(position + 2).and_then(|&digit| hex_value(digit))
        {
            decoded.push(high << 4 | low);
            position += 3;
        } else {
            decoded.push(byte);
            position += 1;
        }
    }
    Cow::Owned(decoded)
}

fn hex_value(digit: u8) -> Option<u8> {
    (digit as char).to_digit(16).map(|value| value as u8)
}

#[cfg(test)]
mod tests {
    use super::percent_decode;

    #[test]
    fn percent_decoding_matches_rawurldecode() {
        assert_eq!(percent_decode(b"plain").as_ref(), b"plain");
        assert_eq!(percent_decode(b"a%20b+c").as_ref(), b"a b+c");
        assert_eq!(percent_decode(b"%C3%A9").as_ref(), "é".as_bytes());
        assert_eq!(percent_decode(b"100%").as_ref(), b"100%");
        assert_eq!(percent_decode(b"%zz%4").as_ref(), b"%zz%4");
    }
}

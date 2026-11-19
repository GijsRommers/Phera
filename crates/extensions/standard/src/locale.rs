//! `setlocale(LC_NUMERIC, 0)` reads the request's current numeric locale.
//!
//! `setlocale(LC_ALL, 'nl_BE.UTF-8')` returns that name and stores it for every
//! category. Acceptance uses locale name syntax so it is independent of the
//! installed libc locales. The only effects today are on subsequent locale
//! queries and CSV character scanning under UTF-8 LC_CTYPE. `localeconv()`
//! and `strcoll()` implement the C/POSIX conventions.
//! Localized collation, decimal and monetary symbols, and time or message
//! formats would need locale data such as ICU's. PHP float-to-string conversion
//! is locale independent, including after setting LC_NUMERIC.

use phera_extension_api::value::PhpString;
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};

use crate::environment::RequestEnvironment;

const BSD_CATEGORIES: bool = cfg!(any(
    target_vendor = "apple",
    target_os = "freebsd",
    target_os = "windows"
));

#[derive(Clone, Copy, PartialEq, Eq)]
enum Category {
    Collate,
    Ctype,
    Monetary,
    Numeric,
    Time,
    Messages,
}

impl Category {
    const ALL: [Self; 6] = [
        Self::Collate,
        Self::Ctype,
        Self::Monetary,
        Self::Numeric,
        Self::Time,
        Self::Messages,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Collate => "LC_COLLATE",
            Self::Ctype => "LC_CTYPE",
            Self::Monetary => "LC_MONETARY",
            Self::Numeric => "LC_NUMERIC",
            Self::Time => "LC_TIME",
            Self::Messages => "LC_MESSAGES",
        }
    }

    fn id(self) -> i64 {
        if BSD_CATEGORIES {
            self as i64 + 1
        } else {
            match self {
                Self::Ctype => 0,
                Self::Numeric => 1,
                Self::Time => 2,
                Self::Collate => 3,
                Self::Monetary => 4,
                Self::Messages => 5,
            }
        }
    }
}

const ALL_ID: i64 = if BSD_CATEGORIES { 0 } else { 6 };

struct RequestLocale {
    categories: [PhpString; Category::ALL.len()],
    /// LC_ALL also sets libc's additional categories on platforms that have them.
    all: PhpString,
}

impl RequestLocale {
    fn new(ctype: PhpString) -> Self {
        let mut categories = std::array::from_fn(|_| "C".into());
        categories[Category::Ctype as usize] = ctype;
        Self {
            categories,
            all: "C".into(),
        }
    }

    fn get(&self, category: Category) -> &PhpString {
        &self.categories[category as usize]
    }

    fn query(&self, category: Option<Category>) -> PhpString {
        if let Some(category) = category {
            return self.get(category).clone();
        }
        let uniform = if BSD_CATEGORIES {
            &self.categories[0]
        } else {
            &self.all
        };
        if self.categories.iter().all(|name| name == uniform) {
            return uniform.clone();
        }
        if BSD_CATEGORIES {
            let names: Vec<_> = self.categories.iter().map(PhpString::as_bytes).collect();
            return names.join(&b'/').into();
        }
        let mut names = Vec::new();
        for category in [
            Category::Ctype,
            Category::Numeric,
            Category::Time,
            Category::Collate,
            Category::Monetary,
            Category::Messages,
        ] {
            names.push((category.name(), self.get(category)));
        }
        for name in [
            "LC_PAPER",
            "LC_NAME",
            "LC_ADDRESS",
            "LC_TELEPHONE",
            "LC_MEASUREMENT",
            "LC_IDENTIFICATION",
        ] {
            names.push((name, &self.all));
        }
        let mut result = Vec::new();
        for (name, value) in names {
            if !result.is_empty() {
                result.push(b';');
            }
            result.extend_from_slice(name.as_bytes());
            result.push(b'=');
            result.extend_from_slice(value.as_bytes());
        }
        result.into()
    }
}

pub(crate) fn register(registry: &mut Registry) {
    registry.constant("LC_ALL", ALL_ID);
    for category in Category::ALL {
        registry.constant(category.name(), category.id());
    }
    registry.function(FunctionEntry::asynchronous(
        "setlocale",
        const {
            &[
                Param::new("category", ParamType::Int),
                Param::new("locales", ParamType::ArrayOrString).nullable(),
                Param::new("rest", ParamType::ArrayOrString)
                    .nullable()
                    .variadic(),
            ]
        },
        setlocale,
    ));
    registry.function(FunctionEntry::sync("localeconv", &[], localeconv));
    registry.function(FunctionEntry::sync(
        "strcoll",
        const {
            &[
                Param::new("string1", ParamType::String),
                Param::new("string2", ParamType::String),
            ]
        },
        strcoll,
    ));
    registry.describe_function("setlocale", "string|false", None);
    registry.describe_function("localeconv", "array", None);
    registry.describe_function("strcoll", "int", None);
}

fn setlocale(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let id = args::int(&arguments, 0).unwrap_or_default();
        let category = Category::ALL
            .into_iter()
            .find(|category| category.id() == id);
        for candidate in arguments.iter().skip(1) {
            if let Value::Array(candidates) = candidate {
                for value in candidates.values() {
                    let name = context.stringify(value).await?;
                    if let Some(selected) = select(context, id, category, &name) {
                        return Ok(Value::String(selected));
                    }
                }
            } else {
                let name = context.stringify(candidate).await?;
                if let Some(selected) = select(context, id, category, &name) {
                    return Ok(Value::String(selected));
                }
            }
        }
        Ok(Value::Bool(false))
    })
}

fn select(
    context: &mut dyn Context,
    id: i64,
    category: Option<Category>,
    name: &PhpString,
) -> Option<PhpString> {
    if name.as_bytes().len() >= 255 {
        context.warning("setlocale(): Specified locale name is too long".into());
        return None;
    }
    if category.is_none() && id != ALL_ID {
        return None;
    }
    let name = name.as_bytes();
    let selected = if name == b"0" {
        None
    } else if c_string(name).is_empty() {
        Some(environment_locale(context, category, Environment::Current))
    } else {
        Some(locale_name(c_string(name))?)
    };
    let ctype = environment_locale(context, Some(Category::Ctype), Environment::Startup);
    let state = context.state().get_or_default::<Option<RequestLocale>>();
    let state = state.get_or_insert_with(|| RequestLocale::new(ctype));
    let Some(selected) = selected else {
        return Some(state.query(category));
    };
    if let Some(category) = category {
        state.categories[category as usize] = selected.clone();
    } else {
        state.categories.fill(selected.clone());
        state.all = selected.clone();
    }
    Some(selected)
}

#[derive(Clone, Copy)]
enum Environment {
    Startup,
    Current,
}

fn environment_locale(
    context: &mut dyn Context,
    category: Option<Category>,
    source: Environment,
) -> PhpString {
    let environment = context.state().get_or_default::<RequestEnvironment>();
    [Some("LC_ALL"), category.map(Category::name), Some("LANG")]
        .into_iter()
        .flatten()
        .filter_map(|name| match source {
            Environment::Startup => environment.startup_value(name.as_bytes()),
            Environment::Current => environment.lookup(name.as_bytes()),
        })
        .find(|name| !name.is_empty())
        .and_then(locale_name)
        .unwrap_or_else(|| "C".into())
}

fn locale_name(name: &[u8]) -> Option<PhpString> {
    if name == b"POSIX" && !BSD_CATEGORIES {
        return Some("C".into());
    }
    if matches!(name, b"C" | b"POSIX" | b"C.UTF-8" | b"C.utf8") {
        return Some(PhpString::from(name));
    }
    let name_text = std::str::from_utf8(name).ok()?;
    let (name_text, modifier) = name_text.split_once('@').unwrap_or((name_text, ""));
    let (name_text, encoding) = name_text.split_once('.').unwrap_or((name_text, ""));
    let (language, territory) = name_text.split_once('_').unwrap_or((name_text, ""));
    let language_valid =
        (2..=3).contains(&language.len()) && language.bytes().all(|byte| byte.is_ascii_lowercase());
    let territory_valid = territory.is_empty()
        || (territory.len() == 2 && territory.bytes().all(|byte| byte.is_ascii_uppercase()));
    let suffix_valid = |part: &str| {
        part.bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    };
    (language_valid && territory_valid && suffix_valid(encoding) && suffix_valid(modifier))
        .then(|| PhpString::from(name))
}

fn localeconv(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    let mut data = PhpArray::new();
    data.insert(ArrayKey::str("decimal_point"), Value::from("."));
    for key in [
        "thousands_sep",
        "int_curr_symbol",
        "currency_symbol",
        "mon_decimal_point",
        "mon_thousands_sep",
        "positive_sign",
        "negative_sign",
    ] {
        data.insert(ArrayKey::str(key), Value::from(""));
    }
    for key in [
        "int_frac_digits",
        "frac_digits",
        "p_cs_precedes",
        "p_sep_by_space",
        "n_cs_precedes",
        "n_sep_by_space",
        "p_sign_posn",
        "n_sign_posn",
    ] {
        data.insert(
            ArrayKey::str(key),
            Value::Int(i64::from(std::ffi::c_char::MAX)),
        );
    }
    for key in ["grouping", "mon_grouping"] {
        data.insert(ArrayKey::str(key), Value::from(PhpArray::new()));
    }
    Ok(Value::from(data))
}

fn strcoll(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = |index| args::string(arguments, index).map_or(&[][..], PhpString::as_bytes);
    let compare = |left, right| i64::from(left) - i64::from(right);
    let left = c_string(string(0));
    let right = c_string(string(1));
    let mismatch = left.iter().zip(right).find(|(left, right)| left != right);
    let difference = mismatch.map_or_else(
        || {
            compare(
                left.get(right.len()).copied().unwrap_or_default(),
                right.get(left.len()).copied().unwrap_or_default(),
            )
        },
        |(&left, &right)| compare(left, right),
    );
    Ok(Value::Int(difference))
}

/// CSV character scanning uses the request's LC_CTYPE encoding.
pub(crate) fn ctype_is_utf8(context: &mut dyn Context) -> bool {
    let category = Category::Ctype;
    let name = context
        .state()
        .get::<Option<RequestLocale>>()
        .and_then(Option::as_ref)
        .map(|state| state.get(category).clone())
        .unwrap_or_else(|| environment_locale(context, Some(category), Environment::Startup));
    let name = name.to_string_lossy().to_ascii_lowercase();
    name.contains("utf-8") || name.contains("utf8")
}

fn c_string(bytes: &[u8]) -> &[u8] {
    bytes.split(|byte| *byte == 0).next().unwrap_or_default()
}

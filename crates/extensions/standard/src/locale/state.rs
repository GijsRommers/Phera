//! `setlocale(LC_TIME, 'C')` changes only the time category of the current request.

use phera_extension_api::{Context, PhpString};

use crate::environment::RequestEnvironment;

const BSD_CATEGORIES: bool = cfg!(any(
    target_vendor = "apple",
    target_os = "freebsd",
    target_os = "windows"
));

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Category {
    Collate,
    Ctype,
    Monetary,
    Numeric,
    Time,
    Messages,
}

impl Category {
    pub(super) const ALL: [Self; 6] = [
        Self::Collate,
        Self::Ctype,
        Self::Monetary,
        Self::Numeric,
        Self::Time,
        Self::Messages,
    ];

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Collate => "LC_COLLATE",
            Self::Ctype => "LC_CTYPE",
            Self::Monetary => "LC_MONETARY",
            Self::Numeric => "LC_NUMERIC",
            Self::Time => "LC_TIME",
            Self::Messages => "LC_MESSAGES",
        }
    }

    pub(super) fn id(self) -> i64 {
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

pub(super) const ALL_ID: i64 = if BSD_CATEGORIES { 0 } else { 6 };

#[derive(Clone)]
pub(crate) struct RequestLocale {
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

pub(super) fn select(
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

/// CSV character scanning uses the request's LC_CTYPE encoding.
pub(crate) fn ctype_name(context: &mut dyn Context) -> PhpString {
    let category = Category::Ctype;
    context
        .state()
        .get::<Option<RequestLocale>>()
        .and_then(Option::as_ref)
        .map(|state| state.get(category).clone())
        .unwrap_or_else(|| environment_locale(context, Some(category), Environment::Startup))
}

pub(super) fn c_string(bytes: &[u8]) -> &[u8] {
    bytes.split(|byte| *byte == 0).next().unwrap_or_default()
}

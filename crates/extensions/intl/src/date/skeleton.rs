//! Generate localized date and time patterns from UTS #35 field skeletons.
//!
//! ```php
//! IntlDatePatternGenerator::create('fr_FR')->getBestPattern('yMMMdjm');
//! // "d MMM y, HH:mm"
//! ```

use std::rc::Rc;

use icu_locale::Locale;
use phera_extension_api::{
    ClassEntry, ClassHooks, Context, MethodEntry, NativeClone, NativeError, NativeResult, Object,
    Param, ParamType, PhpArray, Registry, Value, args,
};

use super::{Style, pattern, skeleton_fields as fields};
use crate::{error, locale, locale_id};

const CLASS: &str = "IntlDatePatternGenerator";
const LOCALE: &[Param] = &[Param::new("locale", ParamType::String)
    .nullable()
    .optional()];
const SKELETON: &[Param] = &[Param::new("skeleton", ParamType::String)];

#[derive(Clone)]
struct Generator {
    locale: Locale,
    patterns: &'static str,
}

const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::copy::<Generator>()),
    ..ClassHooks::EMPTY
};

pub(super) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        hooks: Some(&HOOKS),
        methods: const {
            &[
                MethodEntry::public("__construct", LOCALE, |cx, object, values| {
                    create(cx, object, values, "IntlDatePatternGenerator::__construct")
                }),
                MethodEntry::public("create", LOCALE, |cx, _, values| {
                    create(cx, None, values, "IntlDatePatternGenerator::create")
                })
                .static_()
                .returning("?IntlDatePatternGenerator"),
                MethodEntry::public("getBestPattern", SKELETON, best_pattern)
                    .returning("string|false"),
            ]
        },
        ..ClassEntry::class(CLASS)
    });
}

fn create(
    context: &mut dyn Context,
    object: Option<&Rc<Object>>,
    values: &[Value],
    site: &str,
) -> NativeResult {
    error::clear(context);
    if args::bytes(values, 0).len() > locale::MAX_LEN {
        return locale::too_long_for(context, site, object.is_some());
    }
    let locale = locale_id::cldr(&locale::effective(context, args::bytes(values, 0)));
    let patterns = locale_patterns(&locale);
    let state = Generator { locale, patterns };
    if let Some(object) = object {
        let mut payload = object.native_payload_mut().map_err(|_| missing())?;
        if payload.is_some() {
            error::store(context, site, 1, "Cannot call constructor twice");
            return Err(NativeError::throw(
                "IntlException",
                format!("{site}(): Cannot call constructor twice"),
            ));
        }
        *payload = Some(Box::new(state));
        return Ok(Value::Null);
    }
    let object = context.create_object(CLASS, PhpArray::new())?;
    if let Value::Object(object) = &object {
        *object.native_payload_mut().map_err(|_| missing())? = Some(Box::new(state));
    }
    Ok(object)
}

fn best_pattern(
    context: &mut dyn Context,
    object: Option<&Rc<Object>>,
    values: &[Value],
) -> NativeResult {
    const SITE: &str = "IntlDatePatternGenerator::getBestPattern";
    error::clear(context);
    let Ok(skeleton) = std::str::from_utf8(args::bytes(values, 0)) else {
        return error::fail(
            context,
            SITE,
            10,
            "Skeleton is not a valid UTF-8 string",
            Value::Bool(false),
        );
    };
    let payload = object
        .ok_or_else(missing)?
        .native_payload()
        .map_err(|_| missing())?;
    let state = payload
        .as_deref()
        .and_then(|payload| payload.downcast_ref::<Generator>())
        .ok_or_else(missing)?;
    Ok(Value::from(state.best(skeleton)))
}

fn locale_patterns(locale: &Locale) -> &'static str {
    let entries = include_str!("../../data/cldr-date-skeletons.tsv");
    let find = |locale| {
        entries.lines().find_map(|line| {
            let (key, patterns) = line.split_once('\t')?;
            (key == locale).then_some(patterns)
        })
    };
    let id = locale.id.to_string();
    let mut candidate = id.as_str();
    loop {
        if let Some(patterns) = find(candidate) {
            return patterns;
        }
        let Some((parent, _)) = candidate.rsplit_once('-') else {
            return find("und").unwrap_or_default();
        };
        candidate = parent;
    }
}

impl Generator {
    fn best(&self, skeleton: &str) -> String {
        let mut requested = fields::read(skeleton);
        if requested.is_empty() {
            return String::new();
        }
        let mut suppress_period = false;
        if let Some(hour) = requested
            .iter_mut()
            .find(|field| matches!(field.symbol, 'j' | 'J' | 'C'))
        {
            suppress_period = hour.symbol == 'J';
            let short = pattern::select(&self.locale, Style::None, Style::Short);
            hour.symbol = fields::read(&short)
                .into_iter()
                .find(|field| matches!(field.symbol, 'h' | 'H' | 'K' | 'k'))
                .map_or('H', |field| field.symbol);
            hour.width = 1;
        }
        let output = if let Some(pattern) = self.closest(&requested) {
            fields::adjust(&pattern, &requested)
        } else {
            let split = requested.partition_point(|field| !field.is_time());
            let (date, time) = requested.split_at(split);
            let date_pattern = self.component(date);
            let time_pattern = self.component(time);
            if date.is_empty() || time.is_empty() {
                date_pattern + &time_pattern
            } else {
                pattern::join(&self.locale, fields::date_style(date))
                    .replace("{1}", &date_pattern)
                    .replace("{0}", &time_pattern)
            }
        };
        if suppress_period {
            fields::without_period(&output)
        } else {
            output
        }
    }

    fn component(&self, requested: &[fields::Field]) -> String {
        if let Some(pattern) = self.closest(requested) {
            fields::adjust(&pattern, requested)
        } else {
            requested
                .iter()
                .map(|field| {
                    self.closest(std::slice::from_ref(field)).map_or_else(
                        || field.text(),
                        |pattern| fields::adjust(&pattern, &[*field]),
                    )
                })
                .collect::<Vec<_>>()
                .join(" ")
        }
    }

    fn closest(&self, requested: &[fields::Field]) -> Option<String> {
        if requested.is_empty() {
            return None;
        }
        let expected = fields::mask(requested);
        let candidates = self.patterns.split('\t').filter_map(|row| {
            let (skeleton, pattern) = row.split_once('=')?;
            let candidate = fields::read(skeleton);
            (fields::mask(&candidate) == expected)
                .then(|| (fields::distance(requested, &candidate), pattern))
        });
        let mut best = candidates
            .min_by_key(|(distance, _)| *distance)
            .map(|(distance, pattern)| (distance, pattern.to_owned()));
        for style in [Style::Full, Style::Long, Style::Medium, Style::Short] {
            let pattern = pattern::select(&self.locale, style, Style::None);
            let candidate = fields::read(&pattern);
            if fields::mask(&candidate) == expected {
                let distance = fields::distance(requested, &candidate);
                if best
                    .as_ref()
                    .is_none_or(|(previous, _)| distance < *previous)
                {
                    best = Some((distance, pattern));
                }
            }
        }
        best.map(|(_, pattern)| pattern)
    }
}

fn missing() -> NativeError {
    NativeError::error("Found unconstructed IntlDatePatternGenerator")
}

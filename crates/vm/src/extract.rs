//! PHP extract modes choose names before copying or binding source array cells.

use crate::Vm;
use phera_extension_api::{NativeError, NativeResult, Param, ParamType};
use phera_value::{ArrayKey, PhpArray, Value};
use std::{cell::RefCell, rc::Rc};

/// PHP distinguishes an undefined compiled local from a completely absent name for prefix extraction.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SymbolStatus {
    Missing,
    Undefined,
    Defined,
}

/// Reference extraction rebinds cells; ordinary extraction assigns their values.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ExtractBinding {
    Copy,
    Reference,
}

/// Extraction settings retain the validated naming and aliasing policy.
struct ExtractSettings {
    mode: ExtractMode,
    prefix: String,
    binding: ExtractBinding,
}

/// A source entry survives writes which may separate the array's copy-on-write storage.
struct ExtractEntry {
    key: ArrayKey,
    value: Value,
}

/// The seven PHP extraction policies; reference binding is orthogonal to naming.
#[derive(Clone, Copy)]
enum ExtractMode {
    Overwrite,
    Skip,
    PrefixSame,
    PrefixAll,
    PrefixInvalid,
    PrefixExisting,
    Existing,
}

impl ExtractMode {
    /// Reject unknown flag bits instead of silently treating them as overwrite.
    fn parse(flags: i64) -> Result<Self, NativeError> {
        match flags & !256 {
            0 => Ok(Self::Overwrite),
            1 => Ok(Self::Skip),
            2 => Ok(Self::PrefixSame),
            3 => Ok(Self::PrefixAll),
            4 => Ok(Self::PrefixInvalid),
            5 => Ok(Self::PrefixExisting),
            6 => Ok(Self::Existing),
            _ => Err(NativeError::value_error(
                "extract(): Argument #2 ($flags) must be a valid extract type",
            )),
        }
    }

    /// Prefix rules apply to the original symbol's existence and identifier validity.
    fn name(self, name: String, status: SymbolStatus, prefix: &str) -> Option<String> {
        let valid = valid_identifier(&name);
        let name = match self {
            Self::Skip if status == SymbolStatus::Defined => return None,
            Self::Existing if status != SymbolStatus::Defined => return None,
            Self::PrefixExisting if status == SymbolStatus::Missing => return None,
            Self::PrefixExisting if status == SymbolStatus::Undefined => name,
            Self::PrefixAll | Self::PrefixExisting => format!("{prefix}_{name}"),
            Self::PrefixSame if status == SymbolStatus::Defined => format!("{prefix}_{name}"),
            Self::PrefixInvalid if !valid => format!("{prefix}_{name}"),
            _ => name,
        };
        valid_identifier(&name).then_some(name)
    }
}

/// PHP identifiers accept high bytes, but require a nonnumeric first byte.
fn valid_identifier(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().enumerate().all(|(index, byte)| {
            byte == b'_'
                || byte.is_ascii_alphabetic()
                || byte >= 128
                || (index > 0 && byte.is_ascii_digit())
        })
}

impl Vm {
    /// A defined symbol may hold null; undefined slots must not satisfy EXTR_IF_EXISTS.
    fn extracted_symbol_status(&self, name: &str) -> SymbolStatus {
        self.current().map_or(SymbolStatus::Missing, |frame| {
            let register = frame.named_register(name);
            if register.is_none() && self.in_global_scope() && self.globals.get(name).is_some() {
                return SymbolStatus::Defined;
            }
            match register.and_then(|register| frame.slots(&self.registers).get(register.index())) {
                Some(Some(_)) => SymbolStatus::Defined,
                Some(None) => SymbolStatus::Undefined,
                None => SymbolStatus::Missing,
            }
        })
    }

    /// EXTR_REFS promotes each selected source element and binds the extracted local to it.
    pub(crate) fn extract_variables(
        &mut self,
        mut arguments: Vec<Value>,
        dynamic: bool,
    ) -> NativeResult {
        const PARAMETERS: &[Param] = &[
            Param::new("array", ParamType::Array).prefer_ref(),
            Param::new("flags", ParamType::Int).optional(),
            Param::new("prefix", ParamType::String).optional(),
        ];
        if let Err(error) = self.prepare_call_with_trace("extract", PARAMETERS, &mut arguments) {
            self.failed_native = Some(crate::error_traces::NativeTrace::new("extract", &arguments));
            return Err(error);
        }
        let settings = match ExtractSettings::parse(&arguments) {
            Ok(settings) => settings,
            Err(error) => {
                self.failed_native =
                    Some(crate::error_traces::NativeTrace::new("extract", &arguments));
                return Err(error);
            }
        };
        if dynamic {
            self.failed_native = Some(crate::error_traces::NativeTrace::new("extract", &arguments));
            return Err(NativeError::error("Cannot call extract() dynamically"));
        }
        let argument = arguments.first().cloned().unwrap_or_default();
        let mut array = match &*argument.deref() {
            Value::Array(array) => array.clone(),
            _ => {
                return Err(NativeError::type_error(
                    "extract(): Argument #1 ($array) must be of type array",
                ));
            }
        };
        let entries: Vec<_> = array
            .iter()
            .map(|(key, value)| ExtractEntry {
                key,
                value: value.clone(),
            })
            .collect();
        let mut count = 0;
        for entry in entries {
            count += i64::from(self.extract_entry(&mut array, entry, &settings)?);
        }
        if settings.binding == ExtractBinding::Reference
            && let Value::Reference(reference) = argument
        {
            *reference
                .try_borrow_mut()
                .map_err(|_| internal_error!("extract source"))? = Value::Array(array);
        }
        Ok(Value::Int(count))
    }
    /// Bind one selected name, separating only the source array when references are requested.
    fn extract_entry(
        &mut self,
        array: &mut Rc<PhpArray>,
        entry: ExtractEntry,
        settings: &ExtractSettings,
    ) -> Result<bool, NativeError> {
        let ExtractEntry { key, value } = entry;
        let original = match &key {
            ArrayKey::Interned(name) => name.to_string_lossy(),
            ArrayKey::String(name) => name.to_string_lossy(),
            ArrayKey::Int(number) => number.to_string(),
        };
        let Some(name) = settings.mode.name(
            original.clone(),
            self.extracted_symbol_status(&original),
            &settings.prefix,
        ) else {
            return Ok(false);
        };
        if name == "GLOBALS" {
            return Ok(false);
        }
        let register = self.named_slot(&name)?;
        if settings.binding == ExtractBinding::Reference {
            let reference = if value.is_reference() {
                value
            } else {
                Value::Reference(Rc::new(RefCell::new(value)))
            };
            if let Value::Reference(cell) = &reference {
                self.state.track_cycle_reference(cell);
            }
            Rc::make_mut(array).insert(key, reference.clone());
            self.set(register, reference);
        } else {
            self.assign_local(register, value.deref().clone())?;
        }
        Ok(true)
    }
}

impl ExtractSettings {
    /// Validate the supplied mode before changing any caller variables.
    fn parse(arguments: &[Value]) -> Result<Self, NativeError> {
        let flags = arguments
            .get(1)
            .and_then(|value| {
                if let Value::Int(flags) = &*value.deref() {
                    Some(*flags)
                } else {
                    None
                }
            })
            .unwrap_or(0);
        let mode = ExtractMode::parse(flags)?;
        let prefix = arguments
            .get(2)
            .and_then(Value::str_ref)
            .map_or_else(String::new, |prefix| prefix.to_string_lossy());
        if matches!(
            mode,
            ExtractMode::PrefixSame
                | ExtractMode::PrefixAll
                | ExtractMode::PrefixInvalid
                | ExtractMode::PrefixExisting
        ) && arguments.len() < 3
        {
            return Err(NativeError::value_error(
                "extract(): Argument #3 ($prefix) is required when using this extract type",
            ));
        }
        if !prefix.is_empty() && !valid_identifier(&prefix) {
            return Err(NativeError::value_error(
                "extract(): Argument #3 ($prefix) must be a valid identifier",
            ));
        }
        Ok(Self {
            mode,
            prefix,
            binding: if flags & 256 != 0 {
                ExtractBinding::Reference
            } else {
                ExtractBinding::Copy
            },
        })
    }
}

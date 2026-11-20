//! `unserialize('a:1:{i:0;N;}')` builds an array without using the Rust call stack.

pub(crate) mod graph;
mod objects;
mod options;
mod plain;
mod plain_object;
pub(crate) mod sequence;

use std::{borrow::Cow, rc::Rc};

use phera_extension_api::serialization::{
    ActiveUnserializePolicy, SerializationClass, UnserializeHook, UnserializePolicy,
};
use phera_extension_api::{
    ArrayKey, Context, NativeError, NativeResult, Object, PhpString, Value, args,
};

use crate::wire::{Diagnostic, ParseError, Reader, Token};
use graph::{Content, Graph, Members, Place};

struct Decoder<'a> {
    diagnostic_function: &'a str,
    reader: Reader<'a>,
    graph: Graph,
    frames: Vec<Frame>,
    delayed: Vec<Delayed>,
    options: UnserializePolicy,
    last_class: Option<(&'a [u8], Rc<SerializationClass>)>,
    tracks_references: bool,
}

struct Frame {
    remaining: usize,
    kind: Container,
    members: Members,
}

enum Container {
    Array(Content),
    Properties(Rc<Object>),
    Unserialize(Rc<Object>),
}

enum Delayed {
    Wakeup(Rc<Object>),
    Unserialize(Rc<Object>, Members),
    Legacy(UnserializeHook),
}

enum Failure {
    Parse,
    At(usize),
    Native(NativeError),
}

impl From<ParseError> for Failure {
    fn from(_: ParseError) -> Self {
        Self::Parse
    }
}

impl From<NativeError> for Failure {
    fn from(error: NativeError) -> Self {
        Self::Native(error)
    }
}

type BuildResult<T> = Result<T, Failure>;

pub async fn unserialize(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let Some(input) = args::string(arguments, 0) else {
        return Err(NativeError::type_error(
            "unserialize(): Argument #1 ($data) must be of type string",
        ));
    };
    if input.is_empty() {
        return Ok(Value::Bool(false));
    }
    let options = options::read(context, args::array(arguments, 1).map(Rc::as_ref)).await?;
    if plain::applies(input.as_bytes()) {
        return plain::read(context, input.as_bytes(), &options);
    }
    let mut decoder = Decoder {
        diagnostic_function: "unserialize",
        reader: Reader::new(input.as_bytes()),
        graph: Graph::default(),
        frames: Vec::new(),
        delayed: Vec::new(),
        options,
        last_class: None,
        tracks_references: has_wire_tags(input.as_bytes(), &[b"R:", b"r:", b"C:"]),
    };
    let root = decoder.graph.new_place();
    let build = decoder.build(context, root).await;
    match build {
        Ok(()) => report_extra(context, decoder.reader.offset(), input.len()),
        Err(Failure::Native(error)) => return Err(error),
        Err(Failure::Parse | Failure::At(_)) => {
            let offset = match build {
                Err(Failure::At(offset)) => offset,
                _ => decoder.reader.offset(),
            };
            report_offset(context, offset, input.len());
            if !decoder.delayed.is_empty() {
                decoder.materialize(context)?;
                decoder.run_hooks(context).await?;
            }
            return Ok(Value::Bool(false));
        }
    }
    decoder.materialize(context)?;
    decoder.run_hooks(context).await?;
    let value = decoder.graph.get(root);
    let result = value.try_deref()?.clone();
    Ok(result)
}

/// A tag inside a string may conservatively select the general graph builder.
fn has_wire_tags(bytes: &[u8], tags: &[&[u8; 2]]) -> bool {
    tags.iter()
        .any(|tag| memchr::memmem::find(bytes, *tag).is_some())
}

impl Decoder<'_> {
    fn emit_diagnostic(&mut self, context: &mut dyn Context) {
        if let Some(diagnostic) = self.reader.take_diagnostic() {
            report_wire_diagnostic(context, diagnostic, self.diagnostic_function);
        }
    }

    async fn build(&mut self, context: &mut dyn Context, root: Place) -> BuildResult<()> {
        self.value(context, root).await?;
        while let Some(frame) = self.frames.last() {
            if frame.remaining == 0 {
                let Some(frame) = self.frames.pop() else {
                    return Err(Failure::Parse);
                };
                self.finish_frame(context, frame)?;
                continue;
            }
            let token = self.reader.token(true);
            self.emit_diagnostic(context);
            let Some(frame) = self.frames.last_mut() else {
                return Err(Failure::Parse);
            };
            let key = array_key(token?, &frame.kind)?;
            let key = match &frame.kind {
                Container::Properties(object) => {
                    let key = context
                        .unserialized_property_key(object, key)?
                        .ok_or(Failure::Parse)?;
                    if !frame.members.contains_key(&key) && !object.has(&key) {
                        context.unserialized_dynamic_property(object, &key)?;
                    }
                    key
                }
                _ => key,
            };
            let place = self.graph.member(&mut frame.members, key);
            frame.remaining -= 1;
            self.value(context, place).await?;
        }
        Ok(())
    }

    async fn value(&mut self, context: &mut dyn Context, place: Place) -> BuildResult<()> {
        let start = self.reader.offset();
        let token = self.reader.token(false);
        self.emit_diagnostic(context);
        let token = token?;
        self.graph.enter(
            place,
            self.tracks_references && !matches!(token, Token::Reference(_)),
        );
        let scalar = match token {
            Token::Null => Value::Null,
            Token::Bool(value) => Value::Bool(value),
            Token::Int(value) => Value::Int(value),
            Token::Float(value) => Value::Float(value),
            Token::String(value) => Value::String(php_string(value)),
            Token::Array(0) => {
                self.reader.close()?;
                Value::Array(Rc::default())
            }
            Token::Array(count) => {
                let content = self.graph.start_array(place);
                return self.start_frame(context, count, Container::Array(content));
            }
            Token::Object(class, format) => {
                return self.object(context, place, class, format).await;
            }
            Token::Enum(name) => self.enum_case(context, start, name).await?,
            Token::Reference(slot) | Token::ObjectReference(slot) => {
                return self
                    .graph
                    .reference(place, slot, matches!(token, Token::Reference(_)))
                    .ok_or(Failure::Parse);
            }
        };
        self.graph.value(place, scalar);
        Ok(())
    }

    fn start_frame(
        &mut self,
        context: &mut dyn Context,
        count: usize,
        kind: Container,
    ) -> BuildResult<()> {
        if self.options.depth_exceeded(self.frames.len()) {
            context.warning(self.options.depth_warning(self.diagnostic_function));
            return Err(Failure::Parse);
        }
        self.frames.push(Frame {
            remaining: count,
            kind,
            members: Members::default(),
        });
        Ok(())
    }

    fn finish_frame(&mut self, context: &dyn Context, frame: Frame) -> BuildResult<()> {
        match frame.kind {
            Container::Array(content) => self.graph.finish_array(content, frame.members),
            Container::Properties(object) => {
                self.graph.finish_object(object.clone(), frame.members);
                if context.has_serialization_method(&object, "__wakeup") {
                    self.delayed.push(Delayed::Wakeup(object));
                }
            }
            Container::Unserialize(object) => self
                .delayed
                .push(Delayed::Unserialize(object, frame.members)),
        }
        Ok(self.reader.close()?)
    }

    fn materialize(&mut self, context: &mut dyn Context) -> Result<(), NativeError> {
        self.graph.materialize(context)?;
        self.graph.retain_array_cycles(context);
        Ok(())
    }

    async fn run_hooks(&mut self, context: &mut dyn Context) -> Result<(), NativeError> {
        let hooks = std::mem::take(&mut self.delayed);
        if hooks.is_empty() {
            return Ok(());
        }
        // PHP gives the delayed magic callbacks an independent decoder.
        let previous = ActiveUnserializePolicy::replace(context, None);
        let result = async {
            for hook in hooks {
                match hook {
                    Delayed::Legacy(hook) => hook.invoke(context).await?,
                    Delayed::Wakeup(object) => {
                        context
                            .call_serialization_method(object, "__wakeup", Vec::new())
                            .await?
                    }
                    Delayed::Unserialize(object, members) => {
                        let argument = Value::Array(Rc::new(self.graph.array(members)));
                        context
                            .call_serialization_method(object, "__unserialize", vec![argument])
                            .await?
                    }
                };
            }
            Ok(())
        }
        .await;
        ActiveUnserializePolicy::replace(context, previous);
        result
    }
}

fn report_offset(context: &mut dyn Context, offset: usize, length: usize) {
    context.warning(format!(
        "unserialize(): Error at offset {offset} of {length} bytes"
    ));
}

fn report_extra(context: &mut dyn Context, offset: usize, length: usize) {
    if offset < length {
        context.warning(format!(
            "unserialize(): Extra data starting at offset {offset} of {length} bytes"
        ));
    }
}

fn php_string(bytes: Cow<'_, [u8]>) -> PhpString {
    PhpString::from(bytes.into_owned())
}

fn array_key(token: Token<'_>, container: &Container) -> BuildResult<ArrayKey> {
    match (token, container) {
        (Token::Int(key), Container::Properties(_)) => Ok(ArrayKey::str(key.to_string())),
        (Token::Int(key), _) => Ok(ArrayKey::Int(key)),
        (Token::String(key), Container::Properties(_)) => Ok(ArrayKey::str(php_string(key))),
        (Token::String(key), _) => Ok(ArrayKey::from_string(php_string(key))),
        _ => Err(Failure::Parse),
    }
}

fn emit_diagnostic(context: &mut dyn Context, reader: &mut Reader<'_>) {
    if let Some(diagnostic) = reader.take_diagnostic() {
        report_wire_diagnostic(context, diagnostic, "unserialize");
    }
}

#[cold]
fn report_wire_diagnostic(context: &mut dyn Context, diagnostic: Diagnostic, function: &str) {
    match diagnostic {
        Diagnostic::UnexpectedEnd => {
            context.warning(format!("{function}(): Unexpected end of serialized data"))
        }
        Diagnostic::NumberOutOfRange => {
            context.warning(format!("{function}(): Numerical result out of range"))
        }
        Diagnostic::EscapedString => context.deprecated(format!(
            "{function}(): Unserializing the 'S' format is deprecated"
        )),
        Diagnostic::BadObjectData => context.warning("Bad unserialize data".into()),
        Diagnostic::InsufficientCustomData {
            required,
            present,
            length_overflow,
        } => {
            if length_overflow {
                context.warning(format!("{function}(): Numerical result out of range"));
            }
            context.warning(format!(
                "Insufficient data for unserializing - {required} required, {present} present"
            ));
        }
    }
}

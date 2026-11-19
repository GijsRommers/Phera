//! `session_decode('a|i:1;b|R:1;')` shares references across the two values.

use phera_extension_api::{ArrayKey, Context, NativeError, PhpArray, PhpString, Value};

use super::{Decoder, Failure, Graph, Options, Reader, has_wire_tags};

pub enum Decoded {
    Complete,
    Invalid,
    HookFailed(NativeError),
}

type Publish = fn(&mut dyn Context, Value) -> Result<(), NativeError>;

pub async fn decode_value(
    context: &mut dyn Context,
    bytes: &[u8],
    function: &str,
    publish: Publish,
) -> Result<Decoded, NativeError> {
    let mut decoder = decoder(context, bytes, function).await?;
    let root = decoder.graph.new_place();
    match decoder.build(context, root).await {
        Ok(()) => {
            materialize(context, &mut decoder)?;
            let hooks = decoder.run_hooks(context).await;
            publish(context, decoder.graph.get(root).try_deref()?.clone())?;
            Ok(hook_status(hooks))
        }
        Err(Failure::Native(error)) => Err(error),
        Err(_) => Ok(Decoded::Invalid),
    }
}

pub async fn decode_entries<'a>(
    context: &mut dyn Context,
    bytes: &'a [u8],
    function: &str,
    mut prefix: impl FnMut(&'a [u8]) -> Option<(PhpString, usize)>,
    publish: Publish,
) -> Result<Decoded, NativeError> {
    let mut decoder = decoder(context, bytes, function).await?;
    let mut entries = Vec::new();
    let mut offset = 0;
    while let Some(rest) = bytes.get(offset..).filter(|rest| !rest.is_empty()) {
        let Some((key, length)) = prefix(rest) else {
            return Ok(Decoded::Invalid);
        };
        let Some(value) = rest.get(length..) else {
            return Ok(Decoded::Invalid);
        };
        decoder.reader = Reader::new(value);
        let place = decoder.graph.new_place();
        match decoder.build(context, place).await {
            Ok(()) => {}
            Err(Failure::Native(error)) => return Err(error),
            Err(_) => return Ok(Decoded::Invalid),
        }
        offset += length + decoder.reader.offset();
        entries.push((ArrayKey::String(key), place));
    }
    materialize(context, &mut decoder)?;
    let mut array = PhpArray::new();
    for (key, place) in entries {
        array.insert(key, decoder.graph.get(place));
    }
    publish(context, Value::from(array))?;
    Ok(hook_status(decoder.run_hooks(context).await))
}

fn materialize(context: &mut dyn Context, decoder: &mut Decoder<'_>) -> Result<(), NativeError> {
    decoder.graph.materialize(context)?;
    decoder.graph.retain_array_cycles(context);
    Ok(())
}

fn hook_status(result: Result<(), NativeError>) -> Decoded {
    match result {
        Ok(()) => Decoded::Complete,
        Err(error) => Decoded::HookFailed(error),
    }
}

async fn decoder<'a>(
    context: &mut dyn Context,
    bytes: &'a [u8],
    function: &'a str,
) -> Result<Decoder<'a>, NativeError> {
    Ok(Decoder {
        diagnostic_function: function,
        reader: Reader::new(bytes),
        graph: Graph::default(),
        frames: Vec::new(),
        delayed: Vec::new(),
        options: Options::read(context, None).await?,
        last_class: None,
        tracks_references: has_wire_tags(bytes, &[b"R:", b"r:", b"C:"]),
    })
}

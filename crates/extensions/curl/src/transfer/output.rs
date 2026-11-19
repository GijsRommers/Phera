//! A `CURLOPT_WRITEFUNCTION` callback must return the byte count it consumed.

use phera_extension_api::{Context, NativeError, Value};

use super::Transfer;
use crate::{
    error::TransferError,
    options::{Output, Settings},
};

pub(super) async fn head(
    transfer: &mut Transfer,
    context: &mut dyn Context,
    lines: Vec<Vec<u8>>,
) -> Result<Result<(), TransferError>, NativeError> {
    for line in lines {
        if let Some(sink) = transfer.settings.header.clone()
            && let Err(error) = write(transfer, context, &sink, &line, true).await?
        {
            return Ok(Err(error));
        }
        if transfer.settings.include_header {
            let sink = transfer.settings.output.clone();
            if let Err(error) = write(transfer, context, &sink, &line, true).await? {
                return Ok(Err(error));
            }
        }
    }
    progress(transfer, context).await
}

pub(super) async fn body(
    transfer: &mut Transfer,
    context: &mut dyn Context,
    bytes: &[u8],
) -> Result<Result<(), TransferError>, NativeError> {
    let sink = transfer.settings.output.clone();
    if let Err(error) = write(transfer, context, &sink, bytes, false).await? {
        return Ok(Err(error));
    }
    progress(transfer, context).await
}

async fn write(
    transfer: &mut Transfer,
    context: &mut dyn Context,
    sink: &Output,
    bytes: &[u8],
    header: bool,
) -> Result<Result<(), TransferError>, NativeError> {
    let count = match sink {
        Output::Print => {
            context.write(bytes);
            return Ok(Ok(()));
        }
        Output::Return => {
            transfer.content.extend_from_slice(bytes);
            return Ok(Ok(()));
        }
        Output::Stream(stream) => {
            phera_ext_streams::native::write_resource(context, stream, bytes, "curl_exec").await?
        }
        Output::Callback(callback) => {
            context
                .call(
                    callback,
                    vec![
                        Value::Object(transfer.object.clone()),
                        Value::from(bytes.to_vec()),
                    ],
                )
                .await?
        }
    };
    let count = phera_extension_api::value::to_int(&count, context.diagnostics())?;
    if count == bytes.len() as i64 {
        return Ok(Ok(()));
    }
    let kind = if header { "header" } else { "body" };
    Ok(Err(TransferError::new(
        23,
        format!(
            "Failure writing output to destination, passed {} returned {count} ({kind})",
            bytes.len()
        ),
    )))
}

pub(super) async fn progress(
    transfer: &Transfer,
    context: &mut dyn Context,
) -> Result<Result<(), TransferError>, NativeError> {
    let Some(callback) = transfer
        .settings
        .progress
        .as_ref()
        .filter(|_| !transfer.settings.no_progress)
    else {
        return Ok(Ok(()));
    };
    let numbers = {
        let easy = transfer.handle.borrow();
        [
            easy.info.download_length.unwrap_or(0),
            easy.info.downloaded,
            easy.info.upload_length.unwrap_or(0),
            easy.info.uploaded,
        ]
    };
    let args = std::iter::once(Value::Object(transfer.object.clone()))
        .chain(numbers.map(Value::Int))
        .collect();
    let value = context.call(callback, args).await?;
    if phera_extension_api::value::to_int(&value, context.diagnostics())? != 0 {
        return Ok(Err(TransferError::new(42, "Callback aborted")));
    }
    Ok(Ok(()))
}

pub(super) async fn trace(
    context: &mut dyn Context,
    settings: &Settings,
    line: &str,
) -> Result<(), NativeError> {
    if let Some(stream) = &settings.stderr {
        phera_ext_streams::native::write_resource(context, stream, line.as_bytes(), "curl_exec")
            .await?;
    } else {
        use tokio::io::AsyncWriteExt;
        let _ = tokio::io::stderr().write_all(line.as_bytes()).await;
    }
    Ok(())
}

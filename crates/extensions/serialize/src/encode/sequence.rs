//! `session_encode()` can share reference slots across separately framed values.

use phera_extension_api::{Context, NativeError, Value};

use super::{Encoder, Views};

#[derive(Default)]
pub struct SequenceEncoder(Encoder);

impl SequenceEncoder {
    pub async fn push(
        &mut self,
        context: &mut dyn Context,
        prefix: &[u8],
        value: Value,
    ) -> Result<(), NativeError> {
        self.0.output.append(prefix);
        self.0.write_values(context, value, Views::Inline).await?;
        Ok(())
    }

    pub fn finish(self) -> Vec<u8> {
        self.0.output.finish()
    }
}

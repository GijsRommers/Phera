//! `mb_send_mail($to, 'Café', $body)` sends a MIME message using the
//! transfer charset and encodings configured by `mb_language()`.

use phera_ext_process::mail::{
    BODY, HEADERS, Message, PARAMETERS, RECIPIENT, SUBJECT, additional_headers, deliver,
    mixed_newlines, normalize_field,
};
use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeResult, Param, Registry, Value, args,
};

use crate::{
    codec::{Substitute, decode, encode},
    encoding::Encoding,
    mime::{Scheme, encode_header},
    state::Settings,
};

const NAME: &str = "mb_send_mail";

pub(crate) fn register(registry: &mut Registry) {
    const PARAMS: &[Param] = &[
        RECIPIENT,
        SUBJECT,
        BODY,
        HEADERS,
        PARAMETERS.nullable().with_default(ConstantValue::Null),
    ];
    registry.function(FunctionEntry::asynchronous(NAME, PARAMS, send_mail));
    registry.describe_function(NAME, "bool", None);
}

fn send_mail(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let settings = context.state().get_or_default::<Settings>().clone();
        let mut headers = additional_headers(NAME, arguments.get(3))?;
        let newline: &[u8] = if mixed_newlines(context) {
            b"\n"
        } else {
            b"\r\n"
        };
        let encoding = MailEncoding::new(context, &settings, &headers);
        encoding.append_headers(&mut headers, newline);
        let subject = encode_header(
            args::bytes(&arguments, 1),
            &settings,
            encoding.charset,
            encoding.header,
            newline,
            b"Subject: [PHP-jp nnnnnnnn]".len() + newline.len(),
        );
        let mut body = encoding.body(args::bytes(&arguments, 2), &settings);
        if let Some(nul) = body.iter().position(|byte| *byte == 0) {
            body.truncate(nul);
        }
        let message = Message {
            to: normalize_field(args::bytes(&arguments, 0)),
            subject,
            body,
            headers,
        };
        deliver(
            context,
            NAME,
            message,
            args::string(&arguments, 4).map(|s| s.as_bytes()),
        )
        .await
    })
}

/// Keeping the supplied fields controls both MIME generation and conversion.
struct MailEncoding {
    charset: Encoding,
    header: Scheme,
    transfer: Encoding,
    supplied_mime: bool,
    supplied_type: bool,
    supplied_transfer: bool,
}

impl MailEncoding {
    fn new(context: &mut dyn Context, settings: &Settings, headers: &[u8]) -> Self {
        let supplied_type = header(headers, b"content-type");
        let supplied_transfer = header(headers, b"content-transfer-encoding");
        let language = settings.language.mail();
        let mut charset = Encoding::find(language.charset).unwrap_or(Encoding::Utf8);
        if let Some(value) = supplied_type.and_then(declared_charset) {
            charset = Encoding::find(value).unwrap_or_else(|| {
                context.warning(format!(
                    "{NAME}(): Unsupported charset \"{value}\" - will be regarded as ascii"
                ));
                Encoding::Ascii
            });
        }
        let transfer = if let Some(value) = supplied_transfer {
            let value = String::from_utf8_lossy(value);
            match Encoding::find(&value) {
                Some(encoding @ (Encoding::SevenBit | Encoding::EightBit | Encoding::Base64)) => {
                    encoding
                }
                _ => {
                    context.warning(format!(
                        "{NAME}(): Unsupported transfer encoding \"{value}\" - will be regarded as 8bit"
                    ));
                    Encoding::EightBit
                }
            }
        } else {
            Encoding::find(language.body_encoding).unwrap_or(Encoding::SevenBit)
        };
        Self {
            charset,
            header: Scheme::parse(language.header_encoding.as_bytes()),
            transfer,
            supplied_mime: header(headers, b"mime-version").is_some(),
            supplied_type: supplied_type.is_some(),
            supplied_transfer: supplied_transfer.is_some(),
        }
    }

    fn append_headers(&self, headers: &mut Vec<u8>, newline: &[u8]) {
        if headers.last() == Some(&b'\n') {
            headers.pop();
        }
        if headers.last() == Some(&b'\r') {
            headers.pop();
        }
        let mut append = |field: &[u8]| {
            if !headers.is_empty() {
                headers.extend_from_slice(newline);
            }
            headers.extend_from_slice(field);
        };
        if !self.supplied_mime {
            append(b"MIME-Version: 1.0");
        }
        if !self.supplied_type {
            let field = format!(
                "Content-Type: text/plain; charset={}",
                self.charset.mime_name()
            );
            append(field.as_bytes());
        }
        if !self.supplied_transfer {
            let field = format!("Content-Transfer-Encoding: {}", self.transfer.mime_name());
            append(field.as_bytes());
        }
    }

    fn body(&self, source: &[u8], settings: &Settings) -> Vec<u8> {
        let replacement = Substitute::default();
        let text = encode(
            &decode(source, settings.internal),
            self.charset,
            &replacement,
        );
        let bytes = decode(&text, Encoding::EightBit);
        encode(&bytes, self.transfer, &replacement)
    }
}

/// MIME settings use the last supplied field with a nonempty value.
fn header<'a>(headers: &'a [u8], name: &[u8]) -> Option<&'a [u8]> {
    let mut value = None;
    for line in headers.split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            break;
        }
        let Some(colon) = line.iter().position(|byte| *byte == b':') else {
            continue;
        };
        let field = line[colon + 1..].trim_ascii_start();
        if line[..colon].eq_ignore_ascii_case(name) && !field.is_empty() {
            value = Some(field);
        }
    }
    value
}

fn declared_charset(value: &[u8]) -> Option<&str> {
    let value = std::str::from_utf8(value).ok()?;
    let (_, parameter) = value.split_once(';')?;
    let mut words = parameter
        .split(['=', ' ', '"', '\t'])
        .filter(|part| !part.is_empty());
    if words.next()?.eq_ignore_ascii_case("charset") {
        words.next()
    } else {
        None
    }
}

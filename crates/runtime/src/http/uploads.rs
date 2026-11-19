//! Receiving a form with files: filling `$_POST` and `$_FILES`.
//!
//! ```php
//! // <input type="file" name="photo"> gives:
//! $_FILES['photo'] === [
//!     'name' => 'beach.jpg',           // without any path the client sent
//!     'full_path' => 'beach.jpg',
//!     'type' => 'image/jpeg',          // the client's claim, not checked
//!     'tmp_name' => '/tmp/phpA1b2C3',  // where the bytes wait
//!     'error' => UPLOAD_ERR_OK,
//!     'size' => 48211,
//! ];
//! // <input type="file" name="docs[]" multiple> gives the same six keys,
//! // each holding a list: $_FILES['docs']['name'][0], ...
//! ```
//!
//! Writing the files happens on a thread for blocking work ([`receive`]);
//! the PHP arrays are built afterwards on the request's own thread
//! ([`Form::into_variables`]), because PHP values never change threads.

use std::io::Write;
use std::path::PathBuf;

use phera_value::{PhpArray, Value};
use tempfile::TempPath;

use super::form_limits::FormLimits;
use super::multipart::{Disposition, Multipart};
use super::query::store_named;

/// The values of `$_FILES[...]['error']`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UploadError {
    Ok = 0,
    /// Larger than the server allows.
    TooLarge = 1,
    /// Larger than the form's own `MAX_FILE_SIZE` field allows.
    TooLargeForForm = 2,
    /// The request ended in the middle of the file.
    CutOff = 3,
    /// The form had the control, but no file was chosen.
    NoFile = 4,
    NoTemporaryDirectory = 6,
    CannotWrite = 7,
}

enum Item {
    Field { name: Vec<u8>, value: Vec<u8> },
    File(Upload),
}

struct Upload {
    field: Vec<u8>,
    filename: Vec<u8>,
    content_type: Vec<u8>,
    temporary: Option<TempPath>,
    error: UploadError,
    size: u64,
}

/// A received form, in plain data that may cross threads.
pub(crate) struct Form {
    items: Vec<Item>,
    pub(super) issue: Option<FormIssue>,
    pub(super) consumed: usize,
}

pub(super) enum FormIssue {
    TooManyFields(i64),
    TooManyParts(i64),
    TooManyFiles,
    GarbledHeaders,
    TemporaryFile,
}

impl FormIssue {
    pub(super) fn message(self) -> String {
        match self {
            Self::TooManyFields(limit) => format!(
                "Input variables exceeded {limit}. To increase the limit change \
                 max_input_vars in php.ini."
            ),
            Self::TooManyParts(limit) => {
                let limit = limit as i32;
                format!(
                    "Multipart body parts limit exceeded {limit}. To increase the limit change \
                     max_multipart_body_parts in php.ini."
                )
            }
            Self::TooManyFiles => {
                "Maximum number of allowable file uploads has been exceeded".into()
            }
            Self::GarbledHeaders => "File Upload Mime headers garbled".into(),
            Self::TemporaryFile => "File upload error - unable to create a temporary file".into(),
        }
    }
}

/// Reads a `multipart/form-data` body and writes every acceptable file to
/// a temporary file. Blocks on the disk: call it from a blocking thread.
pub(super) fn receive(body: &[u8], boundary: &[u8], limits: FormLimits) -> Form {
    let mut items = Vec::new();
    let mut issue = None;
    let mut form_limit = 0;
    let mut files_left = limits.max_file_uploads;
    let part_limit = limits.part_limit();
    let mut parts = 0;
    let mut fields = 0;
    let mut unnamed = 0;
    let mut multipart = Multipart::new(body, boundary);
    while let Some(headers) = multipart.next_headers() {
        let Disposition::Form { name, filename } = headers.disposition else {
            continue;
        };
        parts += 1;
        if parts > part_limit {
            issue = Some(FormIssue::TooManyParts(part_limit));
            break;
        }
        let Some(filename) = filename else {
            let Some(name) = name else {
                issue = Some(FormIssue::GarbledHeaders);
                break;
            };
            let (content, _) = multipart.content(None);
            fields += 1;
            if name.eq_ignore_ascii_case(b"MAX_FILE_SIZE") {
                form_limit = phera_extension_api::ini::integer(content);
            }
            if fields <= limits.max_input_vars {
                items.push(Item::Field {
                    name,
                    value: content.to_vec(),
                });
            } else if fields == limits.max_input_vars.saturating_add(1) {
                issue = Some(FormIssue::TooManyFields(limits.max_input_vars));
            }
            continue;
        };
        if !limits.file_uploads {
            continue;
        }
        if files_left <= 0 {
            if files_left == 0 {
                files_left = -1;
                issue = Some(FormIssue::TooManyFiles);
            }
            continue;
        }
        // PHP numbers the files of parts that give no control name.
        let field = name.unwrap_or_else(|| {
            unnamed += 1;
            (unnamed - 1).to_string().into_bytes()
        });
        if !valid_file_field(&field) {
            continue;
        }
        let size = FileSize {
            ini: limits.upload_max_filesize,
            form: form_limit,
        };
        let (content, complete) = if filename.is_empty() {
            (&[][..], false)
        } else {
            files_left -= 1;
            multipart.content(size.read_limit())
        };
        let upload = store(
            field,
            filename,
            headers.content_type,
            content,
            complete,
            size,
        );
        if upload.error == UploadError::NoTemporaryDirectory {
            issue = Some(FormIssue::TemporaryFile);
        }
        items.push(Item::File(upload));
    }
    Form {
        items,
        issue,
        consumed: multipart.consumed(),
    }
}

fn valid_file_field(name: &[u8]) -> bool {
    let mut open = 0;
    for (index, byte) in name.iter().enumerate() {
        match byte {
            b'[' => open += 1,
            b']' if open > 0 && name.get(index + 1).is_none_or(|&byte| byte == b'[') => open -= 1,
            b']' => return false,
            _ => {}
        }
    }
    open == 0
}

struct FileSize {
    ini: i64,
    form: i64,
}

impl FileSize {
    fn read_limit(&self) -> Option<usize> {
        [
            (self.ini > 0).then_some(self.ini),
            (self.form != 0).then_some(self.form.max(0)),
        ]
        .into_iter()
        .flatten()
        .min()
        .and_then(|limit| usize::try_from(limit).ok())
    }
}

/// Decides what becomes of one file part and writes it if it is accepted.
fn store(
    field: Vec<u8>,
    filename: Vec<u8>,
    content_type: Vec<u8>,
    content: &[u8],
    complete: bool,
    limits: FileSize,
) -> Upload {
    let length = content.len() as i64;
    let error = if filename.is_empty() {
        UploadError::NoFile
    } else if limits.ini > 0 && length > limits.ini {
        UploadError::TooLarge
    } else if length > 0 && limits.form != 0 && length > limits.form {
        UploadError::TooLargeForForm
    } else if !complete {
        UploadError::CutOff
    } else {
        UploadError::Ok
    };
    let written = (error == UploadError::Ok).then(|| write_temporary(content));
    let (temporary, error) = match written {
        Some(Ok(path)) => (Some(path), UploadError::Ok),
        Some(Err(error)) => (None, error),
        None => (None, error),
    };
    // PHP reports no type and no size for a file it did not keep.
    let kept = temporary.is_some();
    Upload {
        field,
        filename,
        content_type: if kept { content_type } else { Vec::new() },
        size: if kept { length as u64 } else { 0 },
        temporary,
        error,
    }
}

/// Writes `content` to a new file `php??????` in the temporary directory,
/// readable by this user only.
fn write_temporary(content: &[u8]) -> Result<TempPath, UploadError> {
    let mut file = tempfile::Builder::new()
        .prefix("php")
        .rand_bytes(6)
        .tempfile()
        .map_err(|_| UploadError::NoTemporaryDirectory)?;
    file.write_all(content)
        .map_err(|_| UploadError::CannotWrite)?;
    file.flush().map_err(|_| UploadError::CannotWrite)?;
    Ok(file.into_temp_path())
}

/// The name without the directories a client may have sent with it, with
/// either kind of slash.
fn base_name(filename: &[u8]) -> &[u8] {
    let after_last_slash = filename
        .iter()
        .rposition(|&byte| byte == b'/' || byte == b'\\')
        .map_or(0, |slash| slash + 1);
    &filename[after_last_slash..]
}

impl Form {
    /// Builds `$_POST` and `$_FILES`, and lists the temporary files.
    pub(crate) fn into_variables(self) -> (PhpArray, PhpArray, Vec<PathBuf>) {
        let mut post = PhpArray::new();
        let mut files = PhpArray::new();
        let mut temporary = Vec::new();
        for item in self.items {
            match item {
                Item::Field { name, value } => store_named(&mut post, &name, Value::from(value)),
                Item::File(upload) => {
                    let path = upload
                        .temporary
                        .as_ref()
                        .map(|path| path.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let entries: [(&[u8], Value); 6] = [
                        (b"name", Value::from(base_name(&upload.filename))),
                        (b"full_path", Value::from(upload.filename.as_slice())),
                        (b"type", Value::from(upload.content_type.as_slice())),
                        (b"tmp_name", Value::from(path.as_str())),
                        (b"error", Value::Int(upload.error as i64)),
                        (b"size", Value::Int(upload.size as i64)),
                    ];
                    for (key, value) in entries {
                        store_named(&mut files, &entry_name(&upload.field, key), value);
                    }
                    if let Some(mut file) = upload.temporary {
                        // The request now owns cleanup. Until here, dropping
                        // a cancelled parse also removes its temporary files.
                        file.disable_cleanup(true);
                        temporary.push(file.to_path_buf());
                    }
                }
            }
        }
        (post, files, temporary)
    }
}

/// Where one of the six entries of a file goes: `photo` and `size` give
/// `photo[size]`; `docs[]` and `size` give `docs[size][]`, so that each
/// entry holds a list.
fn entry_name(field: &[u8], key: &[u8]) -> Vec<u8> {
    let bracket = field
        .iter()
        .position(|&byte| byte == b'[')
        .filter(|_| field.ends_with(b"]"));
    let (base, indexes) = field.split_at(bracket.unwrap_or(field.len()));
    [base, b"[", key, b"]", indexes].concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_six_entries_follow_the_control_name() {
        assert_eq!(entry_name(b"photo", b"size"), b"photo[size]");
        assert_eq!(entry_name(b"docs[]", b"name"), b"docs[name][]");
        assert_eq!(entry_name(b"up[main][x]", b"error"), b"up[error][main][x]");
        // Not an array name after all.
        assert_eq!(entry_name(b"odd[name", b"size"), b"odd[name[size]");
    }

    #[test]
    fn directories_are_cut_from_the_name() {
        assert_eq!(base_name(b"beach.jpg"), b"beach.jpg");
        assert_eq!(base_name(b"C:\\Users\\me\\beach.jpg"), b"beach.jpg");
        assert_eq!(base_name(b"/home/me/a\\b/c.txt"), b"c.txt");
        assert_eq!(base_name(b"dir/"), b"");
    }
}

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
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tempfile::TempPath;

use phera_value::{PhpArray, Value};

use super::multipart::{self, Part};
use super::query::store_named;

/// PHP's `upload_max_filesize`.
const LARGEST_FILE: u64 = 2 * 1024 * 1024;
/// PHP's `max_file_uploads`.
const MOST_FILES: usize = 20;
/// PHP's `max_input_vars`.
const MOST_FIELDS: usize = 1000;

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
}

/// Dropping the request stops its blocking form reader at the next checkpoint.
struct CancelForm(Arc<AtomicBool>);

impl Drop for CancelForm {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

pub(crate) async fn receive(body: Vec<u8>, boundary: Vec<u8>) -> Option<Form> {
    let cancel = CancelForm(Arc::default());
    let cancelled = cancel.0.clone();
    tokio::task::spawn_blocking(move || read_form(&body, &boundary, &cancelled))
        .await
        .ok()
}

/// Reads the parts and writes acceptable uploads on a thread that may block.
fn read_form(body: &[u8], boundary: &[u8], cancelled: &AtomicBool) -> Form {
    let mut items = Vec::new();
    let mut form_limit: Option<u64> = None;
    let mut files = 0;
    let mut fields = 0;
    let mut unnamed = 0;
    for part in multipart::parts(body, boundary, cancelled) {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        let Some(filename) = part.filename.clone() else {
            let Some(name) = part.name else { continue };
            if fields == MOST_FIELDS {
                continue;
            }
            fields += 1;
            // A hidden field by which a form limits its own uploads.
            if name.eq_ignore_ascii_case(b"MAX_FILE_SIZE") {
                form_limit = String::from_utf8_lossy(part.content).trim().parse().ok();
            }
            items.push(Item::Field {
                name,
                value: part.content.to_vec(),
            });
            continue;
        };
        if files == MOST_FILES {
            continue;
        }
        files += 1;
        // PHP numbers the files of parts that give no control name.
        let field = part.name.clone().unwrap_or_else(|| {
            unnamed += 1;
            (unnamed - 1).to_string().into_bytes()
        });
        items.push(Item::File(store(
            field, filename, &part, form_limit, cancelled,
        )));
    }
    Form { items }
}

/// Decides what becomes of one file part and writes it if it is accepted.
fn store(
    field: Vec<u8>,
    filename: Vec<u8>,
    part: &Part<'_>,
    form_limit: Option<u64>,
    cancelled: &AtomicBool,
) -> Upload {
    let length = part.content.len() as u64;
    let error = if filename.is_empty() {
        UploadError::NoFile
    } else if !part.complete {
        UploadError::CutOff
    } else if length > LARGEST_FILE {
        UploadError::TooLarge
    } else if form_limit.is_some_and(|limit| length > limit) {
        UploadError::TooLargeForForm
    } else {
        UploadError::Ok
    };
    let written = (error == UploadError::Ok).then(|| write_temporary(part.content, cancelled));
    let (temporary, error) = match written {
        Some(Ok(path)) => (Some(path), UploadError::Ok),
        Some(Err(_)) => (None, UploadError::CannotWrite),
        None => (None, error),
    };
    // PHP reports no type and no size for a file it did not keep.
    let kept = temporary.is_some();
    Upload {
        field,
        filename,
        content_type: if kept {
            part.content_type.clone()
        } else {
            Vec::new()
        },
        size: if kept { length } else { 0 },
        temporary,
        error,
    }
}

/// Writes `content` to a new file `php??????` in the temporary directory,
/// readable by this user only.
fn write_temporary(content: &[u8], cancelled: &AtomicBool) -> std::io::Result<TempPath> {
    let mut file = tempfile::Builder::new()
        .prefix("php")
        .rand_bytes(6)
        .tempfile()?;
    for chunk in content.chunks(64 * 1024) {
        if cancelled.load(Ordering::Relaxed) {
            return Err(std::io::ErrorKind::Interrupted.into());
        }
        file.write_all(chunk)?;
    }
    file.flush()?;
    // Keep ownership with the form while it is handed to the request.
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
    pub(crate) fn into_variables(self) -> (PhpArray, PhpArray, Vec<TempPath>) {
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
                    temporary.extend(upload.temporary);
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

//! Native file classes use the same paths and status queries as stream functions.
//!
//! ```php
//! (new SplFileInfo('src'))->isDir(); // same query as is_dir('src')
//! ```

use std::os::unix::fs::MetadataExt;

use phera_extension_api::{Context, NativeResult, Value};
use rustix::fs::Access;

use crate::metadata::{Query, real_path};

pub use crate::{
    directory::directory_entries,
    errors::{check_path, reason},
    path_text::{basename, parent},
};

/// File status queries shared with SplFileInfo.
#[derive(Clone, Copy)]
pub enum FileStatus {
    Perms,
    Inode,
    Size,
    Owner,
    Group,
    AccessTime,
    ModificationTime,
    ChangeTime,
    Type,
    Writable,
    Readable,
    Executable,
    IsFile,
    IsDirectory,
    IsLink,
}

impl FileStatus {
    pub fn get(self, context: &mut dyn Context, path: &[u8], method: &str) -> NativeResult {
        let query = match self {
            Self::Perms => Query::Field(|meta| i64::from(meta.mode())),
            Self::Inode => Query::Field(|meta| meta.ino() as i64),
            Self::Size => Query::Field(|meta| meta.len() as i64),
            Self::Owner => Query::Field(|meta| i64::from(meta.uid())),
            Self::Group => Query::Field(|meta| i64::from(meta.gid())),
            Self::AccessTime => Query::Field(MetadataExt::atime),
            Self::ModificationTime => Query::Field(MetadataExt::mtime),
            Self::ChangeTime => Query::Field(MetadataExt::ctime),
            Self::Type => Query::Type,
            Self::Writable => Query::Access(Access::WRITE_OK),
            Self::Readable => Query::Access(Access::READ_OK),
            Self::Executable => Query::Access(Access::EXEC_OK),
            Self::IsFile => Query::IsFile,
            Self::IsDirectory => Query::IsDirectory,
            Self::IsLink => Query::IsLink,
        };
        query.path(context, path, method, true)
    }
}

pub fn canonical_path(context: &mut dyn Context, path: &[u8]) -> Value {
    real_path(context, path)
}

pub fn read_link(context: &mut dyn Context, path: &[u8]) -> std::io::Result<Vec<u8>> {
    crate::mutations::read_link(context, path)
}

/// An open handle owned by a native file object for its entire lifetime.
#[derive(Clone)]
pub struct FileStream(std::rc::Rc<phera_extension_api::value::Resource>);

impl FileStream {
    pub async fn open(
        context: &mut dyn Context,
        function: &str,
        name: &[u8],
        mode: &[u8],
        include_path: bool,
        stream_context: &Value,
    ) -> Result<Self, phera_extension_api::NativeError> {
        use phera_extension_api::{NativeError, value::to_bool};

        if to_bool(&FileStatus::IsDirectory.get(context, name, function)?) {
            return Err(NativeError::throw(
                "LogicException",
                "Cannot use SplFileObject with directories",
            ));
        }
        let parameter = if function == "SplFileInfo::openFile" {
            3
        } else {
            4
        };
        crate::context_options::validate(stream_context, function, parameter)?;
        let stream = crate::wrapper::open_with_errors(
            context,
            function,
            name,
            mode,
            include_path,
            crate::wrapper::OpenErrors::Throw,
        )
        .await?
        .ok_or_else(|| NativeError::error("Stream open returned no handle"))?;
        let value = crate::request::new_resource(context.state(), "stream", stream);
        match value {
            Value::Resource(handle) => Ok(Self(handle)),
            _ => Err(NativeError::error("Stream factory returned no resource")),
        }
    }

    pub fn resource(&self) -> Value {
        Value::Resource(self.0.clone())
    }

    pub fn eof(&self) -> Result<bool, phera_extension_api::NativeError> {
        crate::resource::with_stream(&self.0, |stream| stream.eof)
    }

    pub async fn read_line(
        &self,
        limit: Option<u64>,
    ) -> Result<std::io::Result<Vec<u8>>, phera_extension_api::NativeError> {
        crate::transfer::line(&self.0, limit).await
    }
}

/// The raw line and parsed fields share a read even when a quoted field spans lines.
pub struct CsvRecord {
    pub line: Vec<u8>,
    pub fields: phera_extension_api::PhpArray,
}

impl FileStream {
    pub async fn read_csv(
        &self,
        limit: Option<u64>,
        dialect: phera_ext_standard::csv::Dialect,
    ) -> Result<Option<CsvRecord>, phera_extension_api::NativeError> {
        let policy = crate::resource::with_stream(&self.0, |stream| stream.read_policy(None))?;
        let parsed = crate::resource::operate(&self.0, policy, move |stream| {
            if stream.eof {
                return Ok(None);
            }
            let line = stream.line(limit)?;
            let fields = dialect.decode(line.clone(), || stream.line(None))?;
            Ok::<_, std::io::Error>(Some((line, fields)))
        })
        .await?;
        let Ok(Some((line, fields))) = parsed else {
            return Ok(None);
        };
        Ok(Some(CsvRecord {
            line,
            fields: phera_ext_standard::csv::fields_array(fields)?,
        }))
    }
}

impl FileStream {
    pub async fn read_bytes(
        &self,
        length: u64,
    ) -> Result<std::io::Result<Vec<u8>>, phera_extension_api::NativeError> {
        crate::transfer::read(&self.0, Some(length)).await
    }

    pub async fn write_bytes(&self, context: &mut dyn Context, bytes: &[u8]) -> NativeResult {
        crate::transfer::write(context, &self.0, bytes, "SplFileObject::fwrite").await
    }

    pub async fn truncate(
        &self,
        size: u64,
    ) -> Result<std::io::Result<()>, phera_extension_api::NativeError> {
        crate::resource::operate(
            &self.0,
            crate::io_policy::IoPolicy::Blocking,
            move |stream| stream.truncate(size),
        )
        .await
    }
}

/// Applies fscanf's shared scanner to a line already read by a native file object.
pub fn scan_line(line: &[u8], arguments: &[Value]) -> NativeResult {
    use phera_extension_api::args;

    let mut scan = Vec::with_capacity(arguments.len() + 1);
    scan.push(Value::Null);
    scan.extend_from_slice(arguments);
    let format =
        crate::scan_format::Format::parse(args::bytes(&scan, 1), scan.len().saturating_sub(2))?;
    crate::scan::scan_line(line, format, &scan)
}

/// Reads from the PHP stream provided to another native extension.
pub async fn read_resource(
    resource: &phera_extension_api::value::Resource,
    length: u64,
) -> Result<std::io::Result<Vec<u8>>, phera_extension_api::NativeError> {
    crate::transfer::read(resource, Some(length)).await
}

/// Writes to the PHP stream provided to another native extension.
pub async fn write_resource(
    context: &mut dyn Context,
    resource: &phera_extension_api::value::Resource,
    bytes: &[u8],
    function: &str,
) -> NativeResult {
    crate::transfer::write(context, resource, bytes, function).await
}

/// Bounded contents of a named stream, or its directory status.
pub enum StreamContents {
    Directory,
    Bytes { prefix: Vec<u8>, tail: Vec<u8> },
}

/// Open through PHP's wrappers and inspect bounded portions on the I/O pool.
pub async fn inspect_path(
    context: &mut dyn Context,
    function: &str,
    name: &[u8],
    limit: u64,
    tail_length: u64,
) -> Result<Option<StreamContents>, phera_extension_api::NativeError> {
    let Some(mut stream) = crate::wrapper::open(context, function, name, b"rb", false).await?
    else {
        return Ok(None);
    };
    let contents = crate::file_io::blocking(move || {
        if stream.metadata().is_ok_and(|metadata| metadata.is_dir()) {
            return Ok(StreamContents::Directory);
        }
        inspect(&mut stream, limit, tail_length)
    })
    .await?;
    match contents {
        Ok(contents) => Ok(Some(contents)),
        Err(error) => {
            context.warning(format!("{function}(): {}", reason(&error)));
            Ok(None)
        }
    }
}

/// Probe a seekable PHP stream without consuming the caller's data.
pub async fn inspect_resource(
    resource: &phera_extension_api::value::Resource,
    limit: u64,
) -> Result<Option<StreamContents>, phera_extension_api::NativeError> {
    use std::io::SeekFrom;

    let result = crate::resource::operate(
        resource,
        crate::io_policy::IoPolicy::Blocking,
        move |stream| {
            let position = stream.position;
            let eof = stream.eof;
            if !stream.seekable {
                return inspect(stream, limit, 0);
            }
            stream.seek(SeekFrom::Start(0))?;
            let result = inspect(stream, limit, 0);
            stream.seek(SeekFrom::Start(position))?;
            stream.eof = eof;
            result
        },
    )
    .await?;
    Ok(result.ok())
}

fn inspect(
    stream: &mut crate::stream::Stream,
    limit: u64,
    tail_length: u64,
) -> std::io::Result<StreamContents> {
    use std::io::SeekFrom;

    let prefix = stream.read(Some(limit))?;
    let tail = if stream.seekable && tail_length > 0 && prefix.len() as u64 >= tail_length {
        let offset = i64::try_from(tail_length).map_err(|_| std::io::ErrorKind::InvalidInput)?;
        if stream.seek(SeekFrom::End(-offset)).is_ok() {
            stream.read(Some(tail_length))?
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };
    Ok(StreamContents::Bytes { prefix, tail })
}

pub use crate::context_options::validate as validate_stream_context;

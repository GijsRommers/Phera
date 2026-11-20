//! Native file classes use the same paths and status queries as stream functions.
//!
//! ```php
//! (new SplFileInfo('src'))->isDir(); // same query as is_dir('src')
//! ```

use phera_extension_api::{Context, NativeResult, Value, value::Resource};
use rustix::fs::Access;

use crate::metadata::{Query, StatField, real_path};

pub use crate::{
    directory::directory_entries,
    errors::{check_path, reason},
    path_text::{basename, parent},
};

/// Adapters such as XMLWriter borrow only open byte streams.
pub fn is_stream(resource: &Resource) -> bool {
    crate::resource::with_stream(resource, |_| ()).is_ok()
}

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
        self.query().path(context, path, method, true)
    }

    pub async fn get_async(
        self,
        context: &mut dyn Context,
        path: &[u8],
        method: &str,
    ) -> NativeResult {
        self.query().path_async(context, path, method, true).await
    }

    fn query(self) -> Query {
        match self {
            Self::Perms => Query::Field(StatField::Mode),
            Self::Inode => Query::Field(StatField::Inode),
            Self::Size => Query::Field(StatField::Size),
            Self::Owner => Query::Field(StatField::Uid),
            Self::Group => Query::Field(StatField::Gid),
            Self::AccessTime => Query::Field(StatField::Atime),
            Self::ModificationTime => Query::Field(StatField::Mtime),
            Self::ChangeTime => Query::Field(StatField::Ctime),
            Self::Type => Query::Type,
            Self::Writable => Query::Access(Access::WRITE_OK),
            Self::Readable => Query::Access(Access::READ_OK),
            Self::Executable => Query::Access(Access::EXEC_OK),
            Self::IsFile => Query::IsFile,
            Self::IsDirectory => Query::IsDirectory,
            Self::IsLink => Query::IsLink,
        }
    }
}

pub fn canonical_path(context: &mut dyn Context, path: &[u8]) -> Value {
    real_path(context, path)
}

pub fn read_link(context: &mut dyn Context, path: &[u8]) -> std::io::Result<Vec<u8>> {
    crate::mutations::read_link(context, path)
}

/// SPL's live wrapper directory cursor, separate from the default `opendir()` handle.
#[derive(Clone)]
pub struct DirectoryStream(std::rc::Rc<phera_extension_api::value::Resource>);

impl DirectoryStream {
    pub async fn open_wrapper(
        context: &mut dyn Context,
        name: &[u8],
        method: &str,
    ) -> Result<Option<Self>, phera_extension_api::NativeError> {
        use phera_extension_api::NativeError;

        let Some(wrapper) = crate::user_wrappers::locate(context, name) else {
            return Ok(None);
        };
        let supplied = crate::request::files(context)
            .default_context
            .clone()
            .unwrap_or(Value::Null);
        let Some(directory) =
            crate::user_directory::open(context, wrapper, method, name, supplied).await?
        else {
            return Err(NativeError::throw(
                "UnexpectedValueException",
                format!(
                    "Failed to open directory \"{}\"",
                    String::from_utf8_lossy(name)
                ),
            ));
        };
        let Value::Resource(resource) = crate::user_resource::make_directory(context, directory)
        else {
            return Err(NativeError::error("Directory factory returned no resource"));
        };
        Ok(Some(Self(resource)))
    }

    pub async fn read(&self, context: &mut dyn Context, method: &str) -> NativeResult {
        crate::directory::read(context, &self.0, method).await
    }

    pub async fn rewind(&self, context: &mut dyn Context) -> NativeResult {
        crate::directory::rewind(context, &self.0).await
    }
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

        if to_bool(
            &FileStatus::IsDirectory
                .get_async(context, name, function)
                .await?,
        ) {
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
        if crate::filters::is_filter_uri(name)
            && let Some(value) = crate::filters::open_request_filter(
                context,
                function,
                name,
                mode,
                include_path,
                stream_context,
            )
            .await?
        {
            return match value {
                Value::Resource(handle) => Ok(Self(handle)),
                _ => Err(NativeError::throw(
                    "RuntimeException",
                    "Failed to open stream",
                )),
            };
        }
        if let Some(wrapper) = crate::user_wrappers::locate(context, name) {
            let supplied = if stream_context.is_null() {
                crate::context_options::default_for_wrapper(context)
            } else {
                stream_context.clone()
            };
            let flags = if include_path {
                crate::user_wrappers::USE_PATH
            } else {
                0
            };
            let file =
                crate::user_file::open(context, wrapper, function, name, mode, flags, supplied)
                    .await?
                    .ok_or_else(|| {
                        NativeError::throw("RuntimeException", "Failed to open stream")
                    })?;
            let value = crate::user_resource::make_file(context, file);
            return match value {
                Value::Resource(handle) => Ok(Self(handle)),
                _ => Err(NativeError::error("Stream factory returned no resource")),
            };
        }
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
        if let Some(file) = crate::user_file::resource_file(&self.0) {
            return Ok(file.eof.get() && file.buffer.borrow().is_empty());
        }
        crate::resource::with_stream(&self.0, |stream| stream.eof)
    }

    pub async fn read_line(
        &self,
        context: &mut dyn Context,
        limit: Option<u64>,
    ) -> Result<std::io::Result<Vec<u8>>, phera_extension_api::NativeError> {
        crate::transfer::read_context(context, &self.0, "SplFileObject::fgets", limit, true).await
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
        context: &mut dyn Context,
        function: &str,
        length: u64,
    ) -> Result<std::io::Result<Vec<u8>>, phera_extension_api::NativeError> {
        crate::transfer::read_context(context, &self.0, function, Some(length), false).await
    }

    pub async fn write_bytes(&self, context: &mut dyn Context, bytes: &[u8]) -> NativeResult {
        crate::transfer::write(context, &self.0, bytes, "SplFileObject::fwrite").await
    }

    pub async fn truncate(
        &self,
        context: &mut dyn Context,
        size: u64,
    ) -> Result<std::io::Result<()>, phera_extension_api::NativeError> {
        if let Some(file) = crate::user_file::resource_file(&self.0) {
            let truncated = file
                .truncate(context, "SplFileObject::ftruncate", size)
                .await?;
            return Ok(if truncated {
                Ok(())
            } else {
                Err(std::io::ErrorKind::Other.into())
            });
        }
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

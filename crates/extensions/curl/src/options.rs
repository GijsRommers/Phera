//! `curl_setopt($handle, CURLOPT_RETURNTRANSFER, true)` changes the output sink.

mod set;
mod transport;

use std::rc::Rc;
use std::time::Duration;

use phera_extension_api::{PhpArray, Value, value::Resource};

pub(crate) use set::{set_array, set_option};
pub(crate) use transport::{HttpVersion, IpVersion, TlsVersions, Transport};
pub(crate) use transport::{TlsFiles, TlsFormat};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Method {
    #[default]
    Get,
    Post,
    Put,
    Head,
}

impl Method {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Head => "HEAD",
        }
    }
}

#[derive(Clone)]
pub(crate) enum Body {
    Text(Vec<u8>),
    Multipart(Rc<PhpArray>),
}

#[derive(Clone, Default)]
pub(crate) enum Output {
    #[default]
    Print,
    Return,
    Stream(Rc<Resource>),
    Callback(Value),
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Auth {
    #[default]
    Basic,
    None,
    Bearer,
    BasicOrBearer,
}

#[derive(Clone, Copy)]
pub(crate) struct Protocols {
    pub(crate) http: bool,
    pub(crate) https: bool,
}

impl Default for Protocols {
    fn default() -> Self {
        Self {
            http: true,
            https: true,
        }
    }
}

impl Protocols {
    pub(crate) fn permits(self, scheme: &str) -> bool {
        match scheme {
            "http" => self.http,
            "https" => self.https,
            _ => false,
        }
    }
}

#[derive(Clone)]
pub(crate) struct Settings {
    pub(crate) url: Option<String>,
    pub(crate) method: Method,
    pub(crate) custom_method: Option<String>,
    pub(crate) headers: Vec<Vec<u8>>,
    pub(crate) body: Option<Body>,
    pub(crate) input: Option<Rc<Resource>>,
    pub(crate) input_size: Option<u64>,
    pub(crate) read: Option<Value>,
    pub(crate) output: Output,
    pub(crate) include_header: bool,
    pub(crate) header: Option<Output>,
    pub(crate) progress: Option<Value>,
    pub(crate) no_progress: bool,
    pub(crate) follow: bool,
    pub(crate) max_redirects: i64,
    pub(crate) post_redirect: u8,
    pub(crate) auto_referer: bool,
    pub(crate) unrestricted_auth: bool,
    pub(crate) timeout: Option<Duration>,
    pub(crate) connect_timeout: Option<Duration>,
    pub(crate) low_speed_limit: u64,
    pub(crate) low_speed_time: Duration,
    pub(crate) encoding: Option<String>,
    pub(crate) decode: bool,
    pub(crate) agent: Option<Vec<u8>>,
    pub(crate) referer: Option<Vec<u8>>,
    pub(crate) cookie: Option<Vec<u8>>,
    pub(crate) cookie_files: Vec<Vec<u8>>,
    pub(crate) cookie_jar: Option<Vec<u8>>,
    pub(crate) cookie_session: bool,
    pub(crate) username: Option<Vec<u8>>,
    pub(crate) password: Vec<u8>,
    pub(crate) bearer: Option<Vec<u8>>,
    pub(crate) auth: Auth,
    pub(crate) transport: Transport,
    pub(crate) port: Option<u16>,
    pub(crate) fail_on_error: bool,
    pub(crate) file_time: bool,
    pub(crate) max_file_size: u64,
    pub(crate) buffer_size: usize,
    pub(crate) fresh: bool,
    pub(crate) forbid_reuse: bool,
    pub(crate) protocols: Protocols,
    pub(crate) redirect_protocols: Protocols,
    pub(crate) default_protocol: String,
    pub(crate) stderr: Option<Rc<Resource>>,
    pub(crate) verbose: bool,
    pub(crate) header_out: bool,
    pub(crate) pool_idle: usize,
    pub(crate) pool_scope: u64,
    pub(crate) multiplex: bool,
    pub(crate) private: Option<Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            url: None,
            method: Method::Get,
            custom_method: None,
            headers: Vec::new(),
            body: None,
            input: None,
            input_size: None,
            read: None,
            output: Output::Print,
            include_header: false,
            header: None,
            progress: None,
            no_progress: true,
            follow: false,
            max_redirects: 20,
            post_redirect: 0,
            auto_referer: false,
            unrestricted_auth: false,
            timeout: None,
            connect_timeout: Some(Duration::from_secs(300)),
            low_speed_limit: 0,
            low_speed_time: Duration::ZERO,
            encoding: None,
            decode: true,
            agent: None,
            referer: None,
            cookie: None,
            cookie_files: Vec::new(),
            cookie_jar: None,
            cookie_session: false,
            username: None,
            password: Vec::new(),
            bearer: None,
            auth: Auth::default(),
            transport: Transport::default(),
            port: None,
            fail_on_error: false,
            file_time: false,
            max_file_size: 0,
            buffer_size: 16 * 1024,
            fresh: false,
            forbid_reuse: false,
            protocols: Protocols::default(),
            redirect_protocols: Protocols::default(),
            default_protocol: "http".to_owned(),
            stderr: None,
            verbose: false,
            header_out: false,
            pool_idle: 8,
            pool_scope: 0,
            multiplex: true,
            private: None,
        }
    }
}

impl Settings {
    pub(crate) fn effective_method(&self) -> &str {
        self.custom_method.as_deref().unwrap_or(self.method.name())
    }

    pub(crate) fn discard_closed_streams(
        &mut self,
        context: &mut dyn phera_extension_api::Context,
        function: &str,
    ) {
        let closed = |stream: &Rc<Resource>| stream.is_closed();
        let mut warn = |option: &str, fallback: &str| {
            context.warning(format!(
                "{function}(): {option} resource has gone away, resetting to {fallback}"
            ));
        };
        if self.stderr.as_ref().is_some_and(closed) {
            self.stderr = None;
            warn("CURLOPT_STDERR", "stderr");
        }
        if matches!(&self.header, Some(Output::Stream(stream)) if closed(stream)) {
            self.header = None;
            warn("CURLOPT_WRITEHEADER", "default");
        }
        if matches!(&self.output, Output::Stream(stream) if closed(stream)) {
            self.output = Output::Print;
            warn("CURLOPT_FILE", "default");
        }
        if self.input.as_ref().is_some_and(closed) {
            self.input = None;
            warn("CURLOPT_INFILE", "default");
        }
    }

    pub(crate) fn normalized_url(&self) -> Option<url::Url> {
        let input = self.url.as_deref()?.trim();
        if input.is_empty() {
            return None;
        }
        if input.contains("://") {
            url::Url::parse(input).ok()
        } else {
            url::Url::parse(&format!("{}://{input}", self.default_protocol)).ok()
        }
    }
}

pub(crate) fn path(
    context: &mut dyn phera_extension_api::Context,
    bytes: &[u8],
) -> std::io::Result<std::path::PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    let name = std::path::Path::new(std::ffi::OsStr::from_bytes(bytes));
    context
        .state()
        .get_or_default::<phera_extension_api::WorkingDirectory>()
        .resolve(name)
}

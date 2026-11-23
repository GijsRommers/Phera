//! Web requests through the runtime, without a socket: a PHP script on disk,
//! a request built by hand, and the response it produces.

use std::cell::RefCell;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use phera_runtime::{HttpRequest, HttpResponse, OutputSink, Runtime};

/// A directory of PHP files that is removed when the test ends.
struct Site {
    root: PathBuf,
}

impl Site {
    fn new(name: &str) -> Site {
        let root =
            std::env::temp_dir().join(format!("phera-runtime-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create site directory");
        Site {
            root: root.canonicalize().expect("canonical site directory"),
        }
    }

    fn file(&self, name: &str, source: &str) -> PathBuf {
        let path = self.root.join(name);
        std::fs::write(&path, source).expect("write script");
        path
    }
}

impl Drop for Site {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn request(method: &str, target: &str) -> HttpRequest {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let address: SocketAddr = "127.0.0.1:8080".parse().expect("address");
    HttpRequest {
        method: method.to_string(),
        target: target.to_string(),
        path: path.to_string(),
        query: query.to_string(),
        protocol: "HTTP/1.1".to_string(),
        headers: vec![("Host".to_string(), b"example.test:8080".to_vec())],
        body: Vec::new(),
        remote_addr: "203.0.113.9:51000".parse().expect("address"),
        local_addr: address,
        https: false,
    }
}

async fn serve(site: &Site, script: &Path, request: HttpRequest) -> HttpResponse {
    Runtime::new()
        .handle_request(script, &site.root, request)
        .await
}

fn body(response: &HttpResponse) -> String {
    String::from_utf8_lossy(&response.body).into_owned()
}

#[tokio::test(flavor = "current_thread")]
async fn a_worker_keeps_inherited_environment_separate_from_request_writes() {
    let site = Site::new("inherited-env");
    let script = site.file(
        "index.php",
        "<?php if (isset($_GET['change'])) $_ENV['APP_ENV'] = 'request'; echo $_ENV['APP_ENV'];",
    );
    let mut environment = phera_dotenv::Environment::default();
    environment.insert(b"APP_ENV".to_vec(), b"inherited".to_vec());
    let worker = Runtime::with_environment(environment).web_worker(0);
    let first = worker
        .handle_request(&script, &site.root, request("GET", "/index.php?change=1"))
        .await;
    let next = worker
        .handle_request(&script, &site.root, request("GET", "/index.php"))
        .await;
    assert_eq!(body(&first), "request");
    assert_eq!(body(&next), "inherited");
}

#[tokio::test(flavor = "current_thread")]
async fn a_script_sees_the_query_and_server_variables() {
    let site = Site::new("query");
    let script = site.file(
        "index.php",
        r#"<?php
echo $_GET['name'], '|', $_GET['tags'][1], '|', $_SERVER['REQUEST_METHOD'], '|';
echo $_SERVER['REQUEST_URI'], '|', $_SERVER['SCRIPT_NAME'], '|', $_SERVER['SERVER_NAME'], '|';
echo $_SERVER['HTTP_HOST'], '|', $_SERVER['REMOTE_ADDR'];
"#,
    );
    let response = serve(
        &site,
        &script,
        request("GET", "/index.php?name=Gijs&tags[]=a&tags[]=b"),
    )
    .await;

    assert_eq!(response.status, 200);
    assert_eq!(
        body(&response),
        "Gijs|b|GET|/index.php?name=Gijs&tags[]=a&tags[]=b|/index.php|example.test|example.test:8080|203.0.113.9"
    );
    assert_eq!(
        response.header("content-type"),
        Some(&b"text/html; charset=UTF-8"[..])
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_form_post_fills_post_and_request() {
    let site = Site::new("post");
    let script = site.file(
        "form.php",
        "<?php echo $_POST['title'], '|', $_REQUEST['title'], '|', $_REQUEST['page'], '|', $_COOKIE['sid'];",
    );
    let mut post = request("POST", "/form.php?page=2&title=from-query");
    post.headers.push((
        "Content-Type".to_string(),
        b"application/x-www-form-urlencoded; charset=UTF-8".to_vec(),
    ));
    post.headers
        .push(("Cookie".to_string(), b"sid=abc123; theme=dark".to_vec()));
    post.body = b"title=Hello+World%21".to_vec();

    let response = serve(&site, &script, post).await;
    assert_eq!(body(&response), "Hello World!|Hello World!|2|abc123");
}

#[tokio::test(flavor = "current_thread")]
async fn headers_and_status_come_from_the_script() {
    let site = Site::new("headers");
    let script = site.file(
        "api.php",
        r#"<?php
header('Content-Type: application/json');
header('X-Powered-By: Phera');
http_response_code(201);
echo '{"ok":true}';
"#,
    );
    let response = serve(&site, &script, request("GET", "/api.php")).await;

    assert_eq!(response.status, 201);
    assert_eq!(
        response.header("Content-Type"),
        Some(&b"application/json"[..])
    );
    assert_eq!(response.header("x-powered-by"), Some(&b"Phera"[..]));
    assert_eq!(body(&response), r#"{"ok":true}"#);
}

#[tokio::test(flavor = "current_thread")]
async fn a_redirect_sets_the_status() {
    let site = Site::new("redirect");
    let script = site.file("old.php", "<?php header('Location: /new.php');");
    let response = serve(&site, &script, request("GET", "/old.php")).await;

    assert_eq!(response.status, 302);
    assert_eq!(response.header("Location"), Some(&b"/new.php"[..]));
}

#[tokio::test(flavor = "current_thread")]
async fn failures_become_error_responses() {
    let site = Site::new("errors");

    let missing = site.root.join("nope.php");
    let response = serve(&site, &missing, request("GET", "/nope.php")).await;
    assert_eq!(response.status, 404);

    let broken = site.file("broken.php", "<?php echo 1 +;");
    let response = serve(&site, &broken, request("GET", "/broken.php")).await;
    assert_eq!(response.status, 500);
    assert!(response.body.is_empty());

    let crashing = site.file("crash.php", "<?php echo 'before'; undefined_function();");
    let response = serve(&site, &crashing, request("GET", "/crash.php")).await;
    assert_eq!(response.status, 200);
    assert_eq!(body(&response), "before");

    let settings = vec![("display_errors".into(), "1".into())];
    let runtime = Runtime::new().with_startup_ini(settings);
    let response = runtime
        .handle_request(&broken, &site.root, request("GET", "/broken.php"))
        .await;
    assert_eq!(response.status, 200);
    assert!(
        body(&response).contains("Parse error"),
        "{}",
        body(&response)
    );

    let response = runtime
        .handle_request(&crashing, &site.root, request("GET", "/crash.php"))
        .await;
    assert_eq!(response.status, 200);
    assert!(body(&response).starts_with("before"), "{}", body(&response));
    assert!(body(&response).contains("Call to undefined function"));
}

/// Each request runs in a VM of its own: a global or a static from one
/// request must not be visible to the next.
#[tokio::test(flavor = "current_thread")]
async fn requests_do_not_share_state() {
    let site = Site::new("isolation");
    let script = site.file(
        "count.php",
        r#"<?php
function hits() { static $count = 0; return ++$count; }
$total = isset($total) ? $total + 1 : 1;
define('SEEN', true);
echo hits(), hits(), '-', $total;
"#,
    );
    let runtime = Runtime::new();
    for _ in 0..3 {
        let response = runtime
            .handle_request(&script, &site.root, request("GET", "/count.php"))
            .await;
        assert_eq!(body(&response), "12-1");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn compiled_code_is_shared_and_output_is_streamed() {
    struct Collect(Rc<RefCell<Vec<u8>>>);

    impl OutputSink for Collect {
        fn write(&mut self, bytes: &[u8]) {
            self.0.borrow_mut().extend_from_slice(bytes);
        }
    }

    let site = Site::new("cache");
    let script = site.file("hello.php", "<?php echo 'hello ', $argv[1];");
    let runtime = Runtime::new();

    let first = runtime.compile_file(&script).await.expect("compiles");
    let second = runtime.compile_file(&script).await.expect("cached");
    assert!(Arc::ptr_eq(&first, &second));

    let streamed = Rc::new(RefCell::new(Vec::new()));
    let result = runtime
        .run_file(
            &script,
            &["hello.php".to_string(), "world".to_string()],
            Some(Box::new(Collect(streamed.clone()))),
        )
        .await
        .expect("runs");
    assert_eq!(result.status, 0);
    assert_eq!(result.output, b"hello world");
    assert_eq!(*streamed.borrow(), b"hello world");
}

/// `mb_internal_encoding('ASCII')` affects one request; the next starts with UTF-8.
#[tokio::test(flavor = "current_thread")]
async fn multibyte_functions_and_settings_run_in_each_request() {
    let site = Site::new("mbstring");
    let script = site.file(
        "text.php",
        r#"<?php
echo mb_internal_encoding(), '|', mb_substitute_character(), '|', mb_language(), '|';
echo mb_strtoupper('Straße'), '|', mb_strlen('👩🏽‍💻'), '|';
mb_parse_str('name=%C3%A9', $result);
echo $result['name'], '|', iconv('UTF-8', 'UTF-8', '日本語');
mb_internal_encoding('ASCII');
mb_substitute_character('none');
mb_language('Japanese');
"#,
    );
    let runtime = Runtime::new();
    for _ in 0..2 {
        let response = runtime
            .handle_request(&script, &site.root, request("GET", "/text.php"))
            .await;
        assert_eq!(body(&response), "UTF-8|63|neutral|STRASSE|4|é|日本語");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn wrapper_registration_and_included_source_belong_to_each_request() {
    let site = Site::new("wrapper-includes");
    let script = site.file(
        "index.php",
        r#"<?php
class RequestSource {
    public $context;
    private $code;
    private $read = false;
    public function stream_open($path, $mode, $flags, &$opened_path) {
        $opened_path = 'virtual-source.php';
        $this->code = '<?php return ' . (int) $_GET['value'] . ';';
        return true;
    }
    public function stream_read($count) {
        if ($this->read) return '';
        $this->read = true;
        return $this->code;
    }
    public function stream_eof() { return $this->read; }
    public function stream_set_option($option, $arg1, $arg2) { return true; }
    public function stream_stat() { return ['size' => strlen($this->code)]; }
}
echo in_array('request-code', stream_get_wrappers(), true) ? 'leaked:' : 'fresh:';
stream_wrapper_register('request-code', RequestSource::class);
echo include 'request-code://source';
"#,
    );
    let worker = Runtime::new().web_worker(0);
    for (value, output) in [(1, "fresh:1"), (2, "fresh:2")] {
        let path = format!("/index.php?value={value}");
        let response = worker
            .handle_request(&script, &site.root, request("GET", &path))
            .await;
        assert_eq!(response.status, 200);
        assert_eq!(body(&response), output);
    }
}

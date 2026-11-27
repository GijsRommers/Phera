//! End-to-end tests of unified `phera serve` + `Phera\Http\Router`:
//! - Direct `phera serve app.php` returning a `Router`
//! - `Phera\Http\serve($router, ...)` CLI handoff to the same `phera serve` engine
//! - Hybrid mode (`[server] routes = "routes.php"` + static files + classic PHP)
//! - True M:N concurrency via per-worker persistent `idle: Vec<Vm>` pools
//! - HTTP/1.1 keep-alive, HTTP/2 (h2c), SQLite `onStart`, 500 recovery, and `exit()` discard.

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::client::conn::{http1, http2};
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::TcpStream;

struct App {
    process: Child,
    stdout: BufReader<ChildStdout>,
    stderr: Option<ChildStderr>,
    address: String,
    root: PathBuf,
}

impl App {
    fn start_serve(name: &str, script: &str, workers: usize, extra_args: &[&str]) -> App {
        let root =
            std::env::temp_dir().join(format!("phera-http-router-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create test directory");
        let file = root.join("app.php");
        std::fs::write(&file, script).expect("write script");
        Self::spawn_serve(root, file, workers, extra_args)
    }

    fn spawn_serve(root: PathBuf, target: PathBuf, workers: usize, extra_args: &[&str]) -> App {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_phera"));
        cmd.arg("serve")
            .arg("--listen")
            .arg("127.0.0.1:0")
            .arg("--workers")
            .arg(workers.to_string());
        for arg in extra_args {
            cmd.arg(arg);
        }
        cmd.arg(&target)
            .current_dir(&root)
            .env_clear()
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        Self::read_banner(cmd, root)
    }

    fn start_cli_handoff(name: &str, script: &str, workers: usize) -> App {
        let root =
            std::env::temp_dir().join(format!("phera-http-cli-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create test directory");
        let file = root.join("app.php");
        std::fs::write(&file, script).expect("write script");
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_phera"));
        cmd.arg(&file)
            .current_dir(&root)
            .env_clear()
            .env("WORKERS", workers.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        Self::read_banner(cmd, root)
    }

    fn read_banner(mut cmd: Command, root: PathBuf) -> App {
        let mut process = cmd.spawn().expect("start phera");
        let mut stdout = BufReader::new(process.stdout.take().expect("stdout"));
        let stderr = process.stderr.take();
        let mut banner = String::new();
        stdout.read_line(&mut banner).expect("read banner");
        let address = banner
            .split("http://")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .unwrap_or_else(|| panic!("no address in banner: {banner:?}"))
            .to_owned();
        App {
            process,
            stdout,
            stderr,
            address,
            root,
        }
    }

    async fn wait(self) -> (i32, String, String) {
        tokio::task::spawn_blocking(move || {
            let mut app = self;
            let mut out = String::new();
            app.stdout.read_to_string(&mut out).expect("read stdout");
            let mut err = String::new();
            if let Some(mut stderr) = app.stderr.take() {
                let _ = stderr.read_to_string(&mut err);
            }
            let status = app.process.wait().expect("wait for phera");
            (status.code().unwrap_or(-1), out, err)
        })
        .await
        .expect("wait task")
    }

    fn terminate(&self) {
        let status = Command::new("kill")
            .args(["-TERM", &self.process.id().to_string()])
            .status()
            .expect("run kill");
        assert!(status.success());
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct Reply {
    status: u16,
    headers: hyper::HeaderMap,
    body: String,
}

impl Reply {
    fn header(&self, name: &str) -> &str {
        self.headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
    }
}

fn build(
    address: &str,
    method: &str,
    target: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> hyper::Request<Full<Bytes>> {
    let mut builder = hyper::Request::builder()
        .method(method)
        .uri(format!("http://{address}{target}"))
        .header("host", address);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    builder
        .body(Full::new(Bytes::from(body.to_owned())))
        .expect("request")
}

async fn read(response: hyper::Response<hyper::body::Incoming>) -> Reply {
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    Reply {
        status,
        headers,
        body: String::from_utf8(body.to_vec()).expect("UTF-8 body"),
    }
}

struct Http1 {
    sender: http1::SendRequest<Full<Bytes>>,
    address: String,
}

impl Http1 {
    async fn connect(address: &str) -> Http1 {
        let stream = TcpStream::connect(address).await.expect("connect");
        let (sender, connection) = http1::handshake(TokioIo::new(stream))
            .await
            .expect("handshake");
        tokio::spawn(async move {
            let _ = connection.await;
        });
        Http1 {
            sender,
            address: address.to_owned(),
        }
    }

    async fn send(
        &mut self,
        method: &str,
        target: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Reply {
        self.sender.ready().await.expect("connection ready");
        let request = build(&self.address, method, target, headers, body);
        read(self.sender.send_request(request).await.expect("send")).await
    }

    async fn get(&mut self, target: &str) -> Reply {
        self.send("GET", target, &[], "").await
    }
}

async fn get(address: &str, target: &str) -> Reply {
    Http1::connect(address).await.get(target).await
}

const ROUTES: &str = r#"<?php
use Phera\Http\{Router, Request, Response};

$router = new Router();
$echo = fn(Request $r) => [
    'method' => $r->method(),
    'path' => $r->path(),
    'uri' => $r->uri(),
    'params' => $r->params(),
];
$router
    ->get('/plaintext', fn() => 'Hello, World!')
    ->get('/json', fn() => Response::json(['message' => 'Hello, World!', 'path' => '/a/b', 'name' => 'Zoë']))
    ->get('/users/{id}', fn(Request $r) => Response::json(['id' => $r->param('id'), 'missing' => $r->param('nope', 'dflt')]))
    ->get('/users/new', fn() => 'the new-user form')
    ->get('/files/*path', fn(Request $r) => $r->param('path'))
    ->get('/query', fn(Request $r) => [
        'all' => $r->query(),
        'page' => $r->query('page'),
        'tags' => $r->query('tags'),
        'missing' => $r->query('missing', 'none'),
    ])
    ->get('/headers', fn(Request $r) => [
        'agent' => $r->header('X-Agent'),
        'repeated' => $r->header('x-repeated'),
        'missing' => $r->header('x-missing', 'none'),
        'listed' => $r->headers()['x-agent'] ?? null,
    ])
    ->post('/echo', fn(Request $r) => Response::text($r->body(), 201, ['X-Length' => (string) strlen($r->body())]))
    ->post('/decode', fn(Request $r) => ['decoded' => $r->json(), 'object' => get_class($r->json(false))])
    ->put('/items/{id}', $echo)
    ->patch('/items/{id}', $echo)
    ->delete('/items/{id}', $echo)
    ->options('/items', fn() => Response::empty(204, ['Allow' => 'GET, OPTIONS']))
    ->head('/only-head', fn() => Response::empty(200, ['X-Head' => 'yes']))
    ->any('/any', $echo)
    ->route('POST', '/route', $echo)
    ->get('/response', fn() => Response::html('<b>hi</b>')
        ->withStatus(202)
        ->withHeader('X-One', '1')
        ->withHeader('Set-Cookie', 'a=1'))
    ->get('/cookies', fn() => Response::text('ok', 200, ['Set-Cookie' => ['a=1', 'b=2']]))
    ->get('/bytes', fn() => Response::bytes("\x00\x01\x02", 'application/x-test'))
    ->get('/nothing', fn() => null)
    ->get('/inspect', function () {
        $r = Response::json(['a' => 1], 201)->withHeader('X-Test', 'yes');
        return [$r->status(), $r->body(), $r->header('content-type'), $r->headers()];
    })
    ->fallback(fn(Request $r) => Response::text('fallback for ' . $r->path(), 404));
return $router;
"#;

#[tokio::test(flavor = "current_thread")]
async fn routes_methods_params_query_headers_and_bodies() {
    let app = App::start_serve("routes", ROUTES, 1, &[]);
    let mut client = Http1::connect(&app.address).await;

    let reply = client.get("/plaintext").await;
    assert_eq!((reply.status, reply.body.as_str()), (200, "Hello, World!"));
    assert_eq!(reply.header("content-type"), "text/plain; charset=utf-8");

    let reply = client.get("/json").await;
    assert_eq!(reply.header("content-type"), "application/json");
    assert_eq!(
        reply.body,
        r#"{"message":"Hello, World!","path":"/a/b","name":"Zoë"}"#
    );

    let reply = client.get("/users/42").await;
    assert_eq!(reply.body, r#"{"id":"42","missing":"dflt"}"#);
    let reply = client.get("/users/a%20b").await;
    assert_eq!(reply.body, r#"{"id":"a b","missing":"dflt"}"#);
    assert_eq!(client.get("/users/new").await.body, "the new-user form");
    assert_eq!(client.get("/files/css/app.css").await.body, "css/app.css");

    let reply = client
        .get("/query?page=2&tags[]=a&tags[]=b&empty=&x+y=1")
        .await;
    assert_eq!(
        reply.body,
        r#"{"all":{"page":"2","tags":["a","b"],"empty":"","x_y":"1"},"page":"2","tags":["a","b"],"missing":"none"}"#
    );

    let reply = client
        .send(
            "GET",
            "/headers",
            &[
                ("x-agent", "tester"),
                ("x-repeated", "1"),
                ("x-repeated", "2"),
            ],
            "",
        )
        .await;
    assert_eq!(
        reply.body,
        r#"{"agent":"tester","repeated":"1, 2","missing":"none","listed":"tester"}"#
    );

    let reply = client.send("POST", "/echo", &[], "some body").await;
    assert_eq!((reply.status, reply.body.as_str()), (201, "some body"));
    assert_eq!(reply.header("x-length"), "9");

    let reply = client
        .send("POST", "/decode", &[], r#"{"a":[1,2,{"b":null}]}"#)
        .await;
    assert_eq!(
        reply.body,
        r#"{"decoded":{"a":[1,2,{"b":null}]},"object":"stdClass"}"#
    );

    for method in ["PUT", "PATCH", "DELETE"] {
        let reply = client.send(method, "/items/7?x=1", &[], "").await;
        assert_eq!(
            reply.body,
            format!(
                r#"{{"method":"{method}","path":"/items/7","uri":"/items/7?x=1","params":{{"id":"7"}}}}"#
            )
        );
    }
    let reply = client.send("OPTIONS", "/items", &[], "").await;
    assert_eq!(reply.status, 204);
    assert_eq!(reply.header("allow"), "GET, OPTIONS");

    let reply = client.send("HEAD", "/only-head", &[], "").await;
    assert_eq!((reply.status, reply.header("x-head")), (200, "yes"));
    let reply = client.send("HEAD", "/plaintext", &[], "").await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("content-length"), "13");
    assert_eq!(reply.body, "");

    for method in ["GET", "POST", "DELETE"] {
        let reply = client.send(method, "/any", &[], "").await;
        assert!(reply.body.contains(&format!(r#""method":"{method}""#)));
    }
    assert_eq!(client.send("POST", "/route", &[], "").await.status, 200);

    let reply = client.send("POST", "/plaintext", &[], "").await;
    assert_eq!(reply.status, 405);
    assert_eq!(reply.header("allow"), "GET, HEAD");
    let reply = client.send("GET", "/route", &[], "").await;
    assert_eq!((reply.status, reply.header("allow")), (405, "POST"));

    let reply = client.get("/nowhere").await;
    assert_eq!(
        (reply.status, reply.body.as_str()),
        (404, "fallback for /nowhere")
    );

    let reply = client.get("/response").await;
    assert_eq!((reply.status, reply.body.as_str()), (202, "<b>hi</b>"));
    assert_eq!(reply.header("content-type"), "text/html; charset=utf-8");
    assert_eq!(reply.header("x-one"), "1");
    let reply = client.get("/cookies").await;
    let cookies: Vec<_> = reply.headers.get_all("set-cookie").iter().collect();
    assert_eq!(cookies, ["a=1", "b=2"]);
    let reply = client.get("/bytes").await;
    assert_eq!(reply.header("content-type"), "application/x-test");
    assert_eq!(reply.body.as_bytes(), b"\x00\x01\x02");
    let reply = client.get("/nothing").await;
    assert_eq!((reply.status, reply.body.as_str()), (204, ""));
    let reply = client.get("/inspect").await;
    assert_eq!(
        reply.body,
        r#"[201,"{\"a\":1}","application/json",{"content-type":["application/json"],"x-test":["yes"]}]"#
    );

    app.terminate();
    assert_eq!(app.wait().await.0, 0);
}

#[test]
fn route_registration_errors_are_php_exceptions() {
    let script = r#"<?php
use Phera\Http\Router;
$router = new Router();
$attempts = [
    fn() => $router->get('no-slash', fn() => 1),
    fn() => $router->get('/a/{id', fn() => 1),
    fn() => $router->get('/a/x{id}', fn() => 1),
    fn() => $router->get('/a/*rest/more', fn() => 1),
    fn() => $router->get('/twice', fn() => 1)->get('/twice', fn() => 2),
    fn() => $router->route('BREW', '/coffee', fn() => 1),
    fn() => $router->get('/x', 'no_such_function'),
    fn() => Phera\Http\serve($router, 'not an address'),
    fn() => Phera\Http\serve($router, '127.0.0.1:0', -1),
];
foreach ($attempts as $attempt) {
    try {
        $attempt();
        echo "no error\n";
    } catch (Throwable $e) {
        echo get_class($e), ': ', $e->getMessage(), "\n";
    }
}
echo "done\n";
"#;
    let root = std::env::temp_dir().join(format!("phera-http-reg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    let file = root.join("reg.php");
    std::fs::write(&file, script).expect("write");
    let out = Command::new(env!("CARGO_BIN_EXE_phera"))
        .arg(&file)
        .output()
        .expect("run phera");
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        [
            r#"ValueError: invalid route pattern "no-slash": it must start with "/""#,
            r#"ValueError: invalid route pattern "/a/{id": "{" is not closed"#,
            r#"ValueError: invalid route pattern "/a/x{id}": a {parameter} must be a whole path segment"#,
            r#"ValueError: invalid route pattern "/a/*rest/more": a *wildcard must be the end of the pattern"#,
            "LogicException: Phera\\Http\\Router: route GET /twice is already registered",
            r#"ValueError: Phera\Http\Router::route(): Argument #1 ($method) must be an HTTP method such as "GET" or "ANY", "BREW" given"#,
            "TypeError: Phera\\Http\\Router::get(): Argument #2 ($handler) must be a valid callback, function \"no_such_function\" not found or invalid function name",
            r#"ValueError: Phera\Http\serve(): Argument #2 ($addr) must be "host:port", "not an address" given"#,
            "ValueError: Phera\\Http\\serve(): Argument #3 ($workers) must be greater than or equal to 0",
            "done",
        ]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn failing_handler_answers_500_and_exit_discards_only_that_instance() {
    let script = r#"<?php
use Phera\Http\{Router, Request};
$router = new Router();
$count = 0;
$router
    ->get('/count', function () use (&$count) { return (string) ++$count; })
    ->get('/throw', function () use (&$count) { $count += 100; throw new RuntimeException('kaboom'); })
    ->get('/deep', function () {
        $f = function (int $n) use (&$f) { if ($n === 0) { throw new LogicException('deep'); } return $f($n - 1); };
        try { $f(5); } finally { echo "finally ran\n"; }
    })
    ->get('/error', fn() => undefined_function())
    ->get('/type', fn() => 42)
    ->post('/json', fn(Request $r) => $r->json())
    ->get('/caught', function () {
        try { throw new Exception('inner'); } catch (Exception $e) { return 'caught ' . $e->getMessage(); }
    })
    ->get('/exit', function () { exit(3); });
return $router;
"#;
    let app = App::start_serve("failures", script, 1, &[]);
    let mut client = Http1::connect(&app.address).await;
    assert_eq!(client.get("/count").await.body, "1");
    for target in ["/throw", "/deep", "/error", "/type"] {
        let reply = client.get(target).await;
        assert_eq!(reply.status, 500, "{target}");
        assert_eq!(reply.body, "Internal Server Error");
    }
    let reply = client.send("POST", "/json", &[], "{not json").await;
    assert_eq!(reply.status, 500);
    // The pooled instance kept its state across caught errors.
    assert_eq!(client.get("/count").await.body, "102");
    assert_eq!(client.get("/caught").await.body, "caught inner");

    // `exit()` discards only that pooled VM instance; the worker stays alive and
    // the next request forks a fresh instance starting back at `$count = 0`.
    let reply = client.get("/exit").await;
    assert_eq!(reply.status, 500);
    assert_eq!(client.get("/count").await.body, "1");

    app.terminate();
    let (status, _out, err) = app.wait().await;
    assert_eq!(status, 0);
    assert!(
        err.contains("Fatal error: Uncaught RuntimeException: kaboom"),
        "{err}"
    );
    assert!(err.contains("finally ran"), "{err}");
    assert!(err.contains("Uncaught LogicException: deep"), "{err}");
    assert!(
        err.contains("Call to undefined function undefined_function()"),
        "{err}"
    );
    assert!(
        err.contains(
            "a handler must return Phera\\Http\\Response, string, array or null, int returned"
        ),
        "{err}"
    );
    assert!(
        err.contains("Uncaught JsonException: Syntax error"),
        "{err}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn on_start_keeps_a_sqlite_connection_per_instance() {
    let script = r#"<?php
use Phera\Http\{Router, Request, Response};
$router = new Router();
$db = null;
$insert = null;
$opened = 0;
$router->onStart(function (int $worker) use (&$db, &$insert, &$opened) {
    $opened++;
    $db = new PDO('sqlite::memory:');
    $db->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_EXCEPTION);
    $db->exec('CREATE TABLE notes (id INTEGER PRIMARY KEY, text TEXT NOT NULL)');
    $insert = $db->prepare('INSERT INTO notes (text) VALUES (?)');
});
$router
    ->post('/notes', function (Request $r) use (&$db, &$insert) {
        $insert->execute([$r->json()['text']]);
        return Response::json(['id' => (int) $db->lastInsertId()], 201);
    })
    ->get('/notes/{id}', function (Request $r) use (&$db) {
        $select = $db->prepare('SELECT id, text FROM notes WHERE id = ?');
        $select->execute([$r->param('id')]);
        $row = $select->fetch(PDO::FETCH_ASSOC);
        return $row === false ? Response::json(['error' => 'not found'], 404) : Response::json($row);
    })
    ->get('/opened', function () use (&$opened) { return (string) $opened; });
return $router;
"#;
    let app = App::start_serve("sqlite", script, 1, &[]);
    let mut client = Http1::connect(&app.address).await;
    for (index, text) in ["first", "second", "third"].into_iter().enumerate() {
        let reply = client
            .send("POST", "/notes", &[], &format!(r#"{{"text":"{text}"}}"#))
            .await;
        assert_eq!(reply.status, 201);
        assert_eq!(reply.body, format!(r#"{{"id":{}}}"#, index + 1));
    }
    let reply = client.get("/notes/2").await;
    assert_eq!(reply.body, r#"{"id":2,"text":"second"}"#);
    assert_eq!(client.get("/notes/9").await.status, 404);
    assert_eq!(client.get("/opened").await.body, "1");
    app.terminate();
    assert_eq!(app.wait().await.0, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn single_worker_runs_concurrent_async_handlers_via_vm_pool() {
    let script = r#"<?php
use Phera\Http\{Router, Request};
$router = new Router();
$router->get('/wait/{id}', function (Request $r) {
    usleep(120000); // 120 ms non-blocking async sleep on worker's LocalSet
    return 'done:' . $r->param('id');
});
return $router;
"#;
    let app = App::start_serve("mn-concurrency", script, 1, &[]);
    // Warm up first request so worker boot is complete.
    assert_eq!(get(&app.address, "/wait/0").await.body, "done:0");

    // Fire 4 concurrent 120 ms requests against a SINGLE worker (`--workers 1`).
    // Sequential execution would take >= 480 ms; M:N pooling completes in ~150 ms.
    let started = Instant::now();
    let mut tasks = Vec::new();
    for id in 1..=4 {
        let addr = app.address.clone();
        tasks.push(tokio::spawn(async move {
            (id, get(&addr, &format!("/wait/{id}")).await)
        }));
    }
    for task in tasks {
        let (id, reply) = task.await.expect("task");
        assert_eq!(reply.status, 200);
        assert_eq!(reply.body, format!("done:{id}"));
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_millis(360),
        "4 x 120 ms concurrent requests on 1 worker took {:?} (expected overlap < 360 ms)",
        elapsed
    );

    app.terminate();
    assert_eq!(app.wait().await.0, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn hybrid_mode_combines_router_static_files_and_classic_php() {
    let root = std::env::temp_dir().join(format!("phera-http-hybrid-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    std::fs::write(
        root.join("phera.toml"),
        "[server]\nroutes = \"routes.php\"\n",
    )
    .expect("write toml");
    std::fs::write(
        root.join("routes.php"),
        r#"<?php
use Phera\Http\{Router, Request};
return (new Router())
    ->get('/api/ping', fn() => ['pong' => true])
    ->get('/api/items/{id}', fn(Request $r) => ['item' => $r->param('id')]);
"#,
    )
    .expect("write routes");
    std::fs::write(root.join("hello.txt"), "static asset\n").expect("write static");
    std::fs::write(
        root.join("index.php"),
        r#"<?php echo "classic:" . $_SERVER['REQUEST_URI'];"#,
    )
    .expect("write index");

    let app = App::spawn_serve(root.clone(), root, 1, &[]);
    let mut client = Http1::connect(&app.address).await;

    // 1. Fast router endpoint
    let reply = client.get("/api/ping").await;
    assert_eq!(
        (reply.status, reply.body.as_str()),
        (200, r#"{"pong":true}"#)
    );
    let reply = client.get("/api/items/99").await;
    assert_eq!(
        (reply.status, reply.body.as_str()),
        (200, r#"{"item":"99"}"#)
    );
    // 2. Method mismatch on known route still returns 405
    let reply = client.send("POST", "/api/ping", &[], "").await;
    assert_eq!((reply.status, reply.header("allow")), (405, "GET, HEAD"));
    // 3. Static file falls through cleanly
    let reply = client.get("/hello.txt").await;
    assert_eq!((reply.status, reply.body.as_str()), (200, "static asset\n"));
    // 4. Classic PHP front controller falls through cleanly
    let reply = client.get("/blog/post-1").await;
    assert_eq!(
        (reply.status, reply.body.as_str()),
        (200, "classic:/blog/post-1")
    );
    // 5. `routes.php` itself is excluded from direct serving
    let reply = client.get("/routes.php").await;
    assert_eq!(reply.status, 404);

    app.terminate();
    assert_eq!(app.wait().await.0, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn cli_handoff_via_phera_http_serve_uses_unified_server() {
    let script = r#"<?php
use Phera\Http\{Router, Request};
$router = (new Router())
    ->get('/hello/{name}', fn(Request $r) => 'hi ' . $r->param('name'));
Phera\Http\serve($router, '127.0.0.1:0', (int) getenv('WORKERS'));
"#;
    let app = App::start_cli_handoff("handoff", script, 2);
    let reply = get(&app.address, "/hello/phera").await;
    assert_eq!((reply.status, reply.body.as_str()), (200, "hi phera"));
    app.terminate();
    assert_eq!(app.wait().await.0, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn several_workers_and_http2_streams() {
    let script = r#"<?php
use Phera\Http\{Router, Request};
$worker = -1;
$served = 0;
$router = new Router();
$router->onStart(function (int $index) use (&$worker) { $worker = $index; });
$router->get('/work', function () use (&$worker, &$served) {
    $sum = 0;
    for ($i = 0; $i < 200000; $i++) { $sum += $i; }
    $served++;
    return "$worker:$served";
});
$router->get('/users/{id}', fn(Request $r) => ['id' => $r->param('id')]);
return $router;
"#;
    let app = App::start_serve("workers-h2", script, 4, &[]);

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut workers = HashSet::new();
    while workers.len() < 2 && Instant::now() < deadline {
        let mut tasks = Vec::new();
        for _ in 0..16 {
            let address = app.address.clone();
            tasks.push(tokio::spawn(async move { get(&address, "/work").await }));
        }
        for task in tasks {
            let reply = task.await.expect("request task");
            assert_eq!(reply.status, 200);
            let (worker, _) = reply.body.split_once(':').expect("worker:count");
            let worker: usize = worker.parse().expect("worker number");
            assert!(worker < 4);
            workers.insert(worker);
        }
    }
    assert!(workers.len() > 1, "only workers {workers:?} answered");

    // Concurrent HTTP/2 (h2c) streams
    let stream = TcpStream::connect(&app.address).await.expect("connect");
    let (sender, connection) = http2::Builder::new(TokioExecutor::new())
        .handshake(TokioIo::new(stream))
        .await
        .expect("h2 handshake");
    tokio::spawn(async move {
        let _ = connection.await;
    });
    let mut streams = Vec::new();
    for id in 0..16 {
        let mut sender = sender.clone();
        let request = build(&app.address, "GET", &format!("/users/{id}"), &[], "");
        streams.push(tokio::spawn(async move {
            let response = sender.send_request(request).await.expect("h2 send");
            assert_eq!(response.version(), hyper::Version::HTTP_2);
            (id, read(response).await)
        }));
    }
    for stream in streams {
        let (id, reply) = stream.await.expect("stream task");
        assert_eq!(reply.status, 200);
        assert_eq!(reply.body, format!(r#"{{"id":"{id}"}}"#));
    }

    app.terminate();
    assert_eq!(app.wait().await.0, 0);
}

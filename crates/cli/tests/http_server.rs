//! End-to-end tests of `Phera\Http\Server`: a script run with the `phera`
//! binary serves real HTTP/1.1 and HTTP/2 connections.
//!
//! Every script listens on port 0 and prints `listening on http://ADDR` as
//! its first line; the test reads the address from there.

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::client::conn::{http1, http2};
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::TcpStream;

/// A running `phera script.php`.
struct App {
    process: Child,
    stdout: BufReader<ChildStdout>,
    address: String,
    root: PathBuf,
}

impl App {
    fn start(name: &str, script: &str, workers: usize) -> App {
        let root =
            std::env::temp_dir().join(format!("phera-http-server-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create test directory");
        let file = root.join("app.php");
        std::fs::write(&file, script).expect("write script");
        let mut process = Command::new(env!("CARGO_BIN_EXE_phera"))
            .arg(&file)
            .current_dir(&root)
            .env_clear()
            .env("WORKERS", workers.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("start phera");
        let mut stdout = BufReader::new(process.stdout.take().expect("stdout"));
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
            address,
            root,
        }
    }

    /// Waits for the script to end; returns its exit status and the rest
    /// of what it printed. Waits on a blocking thread, so that the test's
    /// own connections keep running and can close when the server ends
    /// them.
    async fn wait(self) -> (i32, String) {
        tokio::task::spawn_blocking(move || {
            let mut app = self;
            let mut rest = String::new();
            app.stdout.read_to_string(&mut rest).expect("read output");
            let status = app.process.wait().expect("wait for phera");
            (status.code().unwrap_or(-1), rest)
        })
        .await
        .expect("wait task")
    }

    /// Stops the server the way an operator does.
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

/// A response as the tests look at it.
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

/// An HTTP/1.1 connection that sends its requests one after another.
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
use Phera\Http\{Server, Request, Response};

$server = new Server('127.0.0.1:0', (int) getenv('WORKERS'));
$echo = fn(Request $r) => [
    'method' => $r->method(),
    'path' => $r->path(),
    'uri' => $r->uri(),
    'params' => $r->params(),
];
$server
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
echo "listening on http://", $server->address(), "\n";
$server->listen();
"#;

#[tokio::test(flavor = "current_thread")]
async fn routes_methods_params_query_headers_and_bodies() {
    let app = App::start("routes", ROUTES, 1);
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
    // HEAD is answered by a GET route, with the GET headers and no body.
    let reply = client.send("HEAD", "/plaintext", &[], "").await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("content-length"), "13");
    assert_eq!(reply.body, "");

    for method in ["GET", "POST", "DELETE"] {
        let reply = client.send(method, "/any", &[], "").await;
        assert!(reply.body.contains(&format!(r#""method":"{method}""#)));
    }
    assert!(client.send("POST", "/route", &[], "").await.status == 200);

    // A path with routes for other methods only: 405 with their list.
    let reply = client.send("POST", "/plaintext", &[], "").await;
    assert_eq!(reply.status, 405);
    assert_eq!(reply.header("allow"), "GET, HEAD");
    let reply = client.send("GET", "/route", &[], "").await;
    assert_eq!((reply.status, reply.header("allow")), (405, "POST"));

    // No route at all: the fallback.
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

#[tokio::test(flavor = "current_thread")]
async fn route_registration_errors_are_php_exceptions() {
    let script = r#"<?php
use Phera\Http\Server;
$server = new Server('127.0.0.1:0', 1);
echo "listening on http://", $server->address(), "\n";
$attempts = [
    fn() => $server->get('no-slash', fn() => 1),
    fn() => $server->get('/a/{id', fn() => 1),
    fn() => $server->get('/a/x{id}', fn() => 1),
    fn() => $server->get('/a/*rest/more', fn() => 1),
    fn() => $server->get('/twice', fn() => 1)->get('/twice', fn() => 2),
    fn() => $server->route('BREW', '/coffee', fn() => 1),
    fn() => $server->get('/x', 'no_such_function'),
    fn() => new Server('not an address'),
    fn() => new Server('127.0.0.1:0', -1),
];
foreach ($attempts as $attempt) {
    try {
        $attempt();
        echo "no error\n";
    } catch (Throwable $e) {
        echo get_class($e), ': ', $e->getMessage(), "\n";
    }
}
$server->stop();
$server->listen();
echo "done\n";
"#;
    let app = App::start("registration", script, 1);
    let (status, output) = app.wait().await;
    assert_eq!(status, 0, "{output}");
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(
        lines,
        [
            r#"ValueError: invalid route pattern "no-slash": it must start with "/""#,
            r#"ValueError: invalid route pattern "/a/{id": "{" is not closed"#,
            r#"ValueError: invalid route pattern "/a/x{id}": a {parameter} must be a whole path segment"#,
            r#"ValueError: invalid route pattern "/a/*rest/more": a *wildcard must be the end of the pattern"#,
            "LogicException: Phera\\Http\\Server: route GET /twice is already registered",
            r#"ValueError: Phera\Http\Server::route(): Argument #1 ($method) must be an HTTP method such as "GET" or "ANY", "BREW" given"#,
            "TypeError: Phera\\Http\\Server::get(): Argument #2 ($handler) must be a valid callback, function \"no_such_function\" not found or invalid function name",
            r#"ValueError: Phera\Http\Server::__construct(): Argument #1 ($addr) must be "host:port", "not an address" given"#,
            "ValueError: Phera\\Http\\Server::__construct(): Argument #2 ($workers) must be greater than or equal to 0",
            "done",
        ]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_failing_handler_answers_500_and_the_worker_goes_on() {
    let script = r#"<?php
use Phera\Http\{Server, Request};
$server = new Server('127.0.0.1:0', 1);
$count = 0;
$server
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
    });
echo "listening on http://", $server->address(), "\n";
$server->listen();
echo "count at the end: $count\n";
"#;
    let app = App::start("failures", script, 1);
    let mut client = Http1::connect(&app.address).await;
    assert_eq!(client.get("/count").await.body, "1");
    for target in ["/throw", "/deep", "/error", "/type"] {
        let reply = client.get(target).await;
        assert_eq!(reply.status, 500, "{target}");
        assert_eq!(reply.body, "Internal Server Error");
    }
    let reply = client.send("POST", "/json", &[], "{not json").await;
    assert_eq!(reply.status, 500);
    // The VM state survived: the counter still counts, and catch works.
    assert_eq!(client.get("/count").await.body, "102");
    assert_eq!(client.get("/caught").await.body, "caught inner");
    app.terminate();
    let (status, output) = app.wait().await;
    assert_eq!(status, 0);
    assert!(
        output.contains("Fatal error: Uncaught RuntimeException: kaboom"),
        "{output}"
    );
    assert!(output.contains("finally ran"), "{output}");
    assert!(output.contains("Uncaught LogicException: deep"), "{output}");
    assert!(
        output.contains("Call to undefined function undefined_function()"),
        "{output}"
    );
    assert!(
        output.contains(
            "a handler must return Phera\\Http\\Response, string, array or null, int returned"
        ),
        "{output}"
    );
    assert!(
        output.contains("Uncaught JsonException: Syntax error"),
        "{output}"
    );
    assert!(output.ends_with("count at the end: 102\n"), "{output}");
}

#[tokio::test(flavor = "current_thread")]
async fn on_worker_start_keeps_a_sqlite_connection_per_worker() {
    let script = r#"<?php
use Phera\Http\{Server, Request, Response};
$server = new Server('127.0.0.1:0', 1);
$db = null;
$insert = null;
$opened = 0;
$server->onWorkerStart(function (int $worker) use (&$db, &$insert, &$opened) {
    $opened++;
    $db = new PDO('sqlite::memory:');
    $db->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_EXCEPTION);
    $db->exec('CREATE TABLE notes (id INTEGER PRIMARY KEY, text TEXT NOT NULL)');
    $insert = $db->prepare('INSERT INTO notes (text) VALUES (?)');
});
$server
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
echo "listening on http://", $server->address(), "\n";
$server->listen();
"#;
    let app = App::start("sqlite", script, 1);
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
    // The callback ran once, before the first request.
    assert_eq!(client.get("/opened").await.body, "1");
    app.terminate();
    assert_eq!(app.wait().await.0, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn several_workers_each_serve_with_their_own_vm() {
    let script = r#"<?php
use Phera\Http\Server;
$server = new Server('127.0.0.1:0', (int) getenv('WORKERS'));
$worker = -1;
$served = 0;
$server->onWorkerStart(function (int $index) use (&$worker) { $worker = $index; });
$server->get('/work', function () use (&$worker, &$served) {
    // Keep this worker's thread busy, so that the others accept too.
    $sum = 0;
    for ($i = 0; $i < 200000; $i++) { $sum += $i; }
    $served++;
    return "$worker:$served";
});
echo "listening on http://", $server->address(), "\n";
$server->listen();
echo "after listen\n";
"#;
    let app = App::start("workers", script, 4);
    // The other workers start once worker 0 listens, each booting the
    // script, so send waves of concurrent requests until several answered.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut workers = HashSet::new();
    while workers.len() < 2 && std::time::Instant::now() < deadline {
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
    app.terminate();
    let (status, output) = app.wait().await;
    assert_eq!(status, 0);
    // Code after listen() runs once: the other workers end with listen().
    assert_eq!(output, "after listen\n");
}

#[tokio::test(flavor = "current_thread")]
async fn http1_keep_alive_and_http2_streams() {
    let app = App::start("protocols", ROUTES, 2);

    // Many requests over one HTTP/1.1 connection.
    let mut client = Http1::connect(&app.address).await;
    for id in 0..20 {
        let reply = client.get(&format!("/users/{id}")).await;
        assert_eq!(reply.body, format!(r#"{{"id":"{id}","missing":"dflt"}}"#));
    }

    // Concurrent streams over one HTTP/2 connection (cleartext, prior
    // knowledge).
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
        assert_eq!(reply.body, format!(r#"{{"id":"{id}","missing":"dflt"}}"#));
    }
    let request = build(&app.address, "POST", "/echo", &[], "over h2");
    let reply = read(sender.clone().send_request(request).await.expect("h2 post")).await;
    assert_eq!((reply.status, reply.body.as_str()), (201, "over h2"));

    app.terminate();
    assert_eq!(app.wait().await.0, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn stop_ends_listen_and_exit_ends_the_program() {
    let script = r#"<?php
use Phera\Http\Server;
$server = new Server('127.0.0.1:0', (int) getenv('WORKERS'));
$server->get('/stop', function () use ($server) { $server->stop(); return 'stopping'; });
$server->get('/exit', function () { echo "exiting\n"; exit(3); });
$server->get('/ping', fn() => 'pong');
echo "listening on http://", $server->address(), "\n";
$server->listen();
echo "listen returned\n";
"#;
    let app = App::start("stop", script, 2);
    assert_eq!(get(&app.address, "/ping").await.body, "pong");
    assert_eq!(get(&app.address, "/stop").await.body, "stopping");
    let (status, output) = app.wait().await;
    assert_eq!((status, output.as_str()), (0, "listen returned\n"));

    let app = App::start("exit", script, 1);
    assert_eq!(get(&app.address, "/ping").await.body, "pong");
    let reply = get(&app.address, "/exit").await;
    assert!(reply.status == 503 || reply.status == 500);
    let (status, output) = app.wait().await;
    // exit() ends the program as it would any script: listen() does not
    // return.
    assert_eq!((status, output.as_str()), (3, "exiting\n"));
}

//! `Phera\Log\Logger`: levels, filtering, JSON and text lines, PSR-3
//! placeholders, `withContext()`, the file writer, and loggers made before
//! `Phera\snapshot()` working in every forked request.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use phera_runtime::{BootReport, HttpRequest, Runtime};

/// A directory removed when the test ends.
struct Directory(PathBuf);

impl Directory {
    fn new(name: &str) -> Directory {
        let path = std::env::temp_dir().join(format!("phera-log-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create test directory");
        Directory(path.canonicalize().expect("canonical directory"))
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs `source` as a CLI script in `directory`; returns what it printed.
fn run(directory: &Path, source: &str) -> String {
    let script = directory.join("script.php");
    std::fs::write(&script, source).expect("write script");
    let result = Runtime::new()
        .run_file_blocking(&script, &[], None)
        .expect("the script runs");
    let output = String::from_utf8_lossy(&result.output).into_owned();
    assert_eq!(result.status, 0, "{output}");
    output
}

/// Each line's timestamp (`"ts":"..."` or `[...]` at the start), which
/// changes from run to run, as `TS`.
fn without_times(text: &str) -> String {
    const TIME: usize = "2026-11-27T09:15:02.123456+00:00".len();
    text.lines()
        .map(|line| {
            for prefix in ["{\"ts\":\"", "["] {
                if let Some(rest) = line.strip_prefix(prefix)
                    && rest.get(26..TIME) == Some("+00:00")
                {
                    return format!("{prefix}TS{}\n", &rest[TIME..]);
                }
            }
            format!("{line}\n")
        })
        .collect()
}

#[test]
fn levels_filtering_formats_and_placeholders() {
    let directory = Directory::new("levels");
    let output = run(
        &directory.0,
        r#"<?php
use Phera\Log\{Logger, Level};
$all = Logger::memory('all');
foreach ([Level::EMERGENCY, Level::ALERT, Level::CRITICAL, Level::ERROR,
          Level::WARNING, Level::NOTICE, Level::INFO, Level::DEBUG] as $level) {
    $all->$level("$level {n}", ['n' => 1]);
}
$all->log('WARNING', 'by name');
echo implode(',', array_column($all->records(), 'level')), "\n";
echo implode(',', array_column($all->records(), 'message')), "\n";

$warnings = Logger::memory('w', 'warning', 'text');
foreach (['emergency', 'error', 'warning', 'notice', 'info', 'debug'] as $level) {
    $warnings->$level($level);
}
echo implode(',', array_column($warnings->records(), 'message')), "\n";
var_dump($warnings->isHandling('warning'), $warnings->isHandling('notice'));

$json = Logger::memory('api');
$json->info('User {user} paid {amount} {currency} ({missing}) {not a name} {}', [
    'user' => 'ada', 'amount' => 12.5, 'currency' => null, 'flags' => [true, false], 7 => 'seven',
]);
$json->error('{0}', ['first']);
$json->notice('no context');
foreach ($json->records() as $record) echo $record['formatted'], "\n";

$text = Logger::memory('txt', 'debug', 'text');
$text->info('with {a}', ['a' => 'b', 'n' => 1]);
$text->debug('bare');
foreach ($text->records() as $record) echo $record['formatted'], "\n";

class Name implements Stringable { function __toString(): string { return 'Ada Lovelace'; } }
class Point implements JsonSerializable { function jsonSerialize(): mixed { return ['x' => 1, 'y' => [2]]; } }
$objects = Logger::memory('obj');
$thrown = new RuntimeException('outer', 7, new LogicException('inner')); $line = __LINE__;
$objects->info('{who} at {when} sees {plain}', [
    'who' => new Name(), 'when' => new DateTimeImmutable('2020-01-02 03:04:05.25 UTC'),
    'plain' => new ArrayObject(), 'point' => new Point(), 'nan' => NAN,
    'exception' => $thrown,
]);
$record = $objects->records()[0];
echo $record['message'], "\n";
echo json_encode($record['context']['point']), ' ', $record['context']['nan'], ' ', $record['context']['plain'], "\n";
$exception = $record['context']['exception'];
echo $exception['class'], ' ', $exception['message'], ' ', $exception['code'], ' ', $exception['line'] === $line ? 'line' : $exception['line'], ' ', $exception['previous']['class'], "\n";

try { $all->log('loud', 'x'); } catch (InvalidArgumentException $e) { echo $e::class, "\n"; }
try { new Logger('x', 'info', 'xml'); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { (new Logger())->records(); } catch (LogicException $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    let expected = r#"emergency,alert,critical,error,warning,notice,info,debug,warning
emergency 1,alert 1,critical 1,error 1,warning 1,notice 1,info 1,debug 1,by name
emergency,error,warning
bool(true)
bool(false)
{"ts":"TS","level":"info","channel":"api","message":"User ada paid 12.5  ({missing}) {not a name} {}","context":{"user":"ada","amount":12.5,"currency":null,"flags":[true,false],"7":"seven"}}
{"ts":"TS","level":"error","channel":"api","message":"first","context":["first"]}
{"ts":"TS","level":"notice","channel":"api","message":"no context","context":{}}
[TS] txt.INFO: with b {"a":"b","n":1}
[TS] txt.DEBUG: bare
Ada Lovelace at 2020-01-02T03:04:05.250000+00:00 sees [object ArrayObject]
{"x":1,"y":[2]} NAN [object ArrayObject]
RuntimeException outer 7 line LogicException
InvalidArgumentException
Phera\Log\Logger: format must be "json" or "text", "xml" given
Phera\Log\Logger::records() needs a logger with target "memory"
"#;
    assert_eq!(without_times(&output), expected);
}

#[test]
fn with_context_returns_a_new_logger_and_leaves_the_old_one() {
    let directory = Directory::new("context");
    let output = run(
        &directory.0,
        r#"<?php
use Phera\Log\Logger;
$base = Logger::memory('app')->withContext(['service' => 'api']);
$request = $base->withContext(['request' => 'r-1', 'service' => 'api-v2']);
$base->info('base', ['extra' => 1]);
$request->info('request', ['service' => 'call wins']);
$request->info('request only');
var_dump($base !== $request);
foreach ($base->records() as $record) echo json_encode($record['context']), "\n";
"#,
    );
    assert_eq!(
        output,
        r#"bool(true)
{"service":"api","extra":1}
{"service":"call wins","request":"r-1"}
{"service":"api-v2","request":"r-1"}
"#
    );
}

#[test]
fn a_file_logger_appends_lines_from_every_logger_of_the_file() {
    let directory = Directory::new("file");
    let output = run(
        &directory.0,
        r#"<?php
use Phera\Log\Logger;
file_put_contents(__DIR__ . '/app.log', "existing\n");
$one = new Logger('one', 'debug', 'json', __DIR__ . '/app.log', ['pid' => 1]);
chdir(__DIR__);
$two = new Logger('two', 'info', 'text', 'app.log');
for ($i = 0; $i < 100; $i++) {
    $one->debug('line {i}', ['i' => $i]);
    $two->info("text $i");
}
$one->flush();
$two->flush();
$lines = file(__DIR__ . '/app.log', FILE_IGNORE_NEW_LINES);
echo count($lines), ' ', $lines[0], "\n";
echo json_decode($lines[1], true)['message'], ' ', json_decode($lines[1], true)['context']['pid'], "\n";
echo substr($lines[200], strpos($lines[200], ']') + 2), "\n";
$three = new Logger('three', 'info', 'json', __DIR__ . '/last.log');
$three->info('written when the script ends');
try { new Logger('x', 'info', 'json', __DIR__ . '/no/such/dir/x.log'); } catch (RuntimeException $e) { echo "cannot open\n"; }
"#,
    );
    assert_eq!(
        output,
        "201 existing\nline 0 1\ntwo.INFO: text 99\ncannot open\n"
    );
    // No flush() for this one: the end of the script writes it.
    let last = std::fs::read_to_string(directory.0.join("last.log")).expect("last.log");
    assert!(
        last.contains(r#""message":"written when the script ends""#),
        "{last}"
    );
}

fn request() -> HttpRequest {
    let address: SocketAddr = "127.0.0.1:8080".parse().expect("address");
    HttpRequest {
        method: "GET".to_string(),
        target: "/index.php".to_string(),
        path: "/index.php".to_string(),
        query: String::new(),
        protocol: "HTTP/1.1".to_string(),
        headers: vec![("Host".to_string(), b"example.test".to_vec())],
        body: Vec::new(),
        remote_addr: "203.0.113.9:51000".parse().expect("address"),
        local_addr: address,
        https: false,
    }
}

/// Loggers made at boot work in each fork: a memory logger's records are
/// each fork's own, and a file logger keeps writing to its file.
#[tokio::test(flavor = "current_thread")]
async fn loggers_made_before_the_snapshot_work_in_every_fork() {
    let directory = Directory::new("snapshot");
    let root = directory.0.clone();
    let script = root.join("index.php");
    std::fs::write(
        &script,
        r#"<?php
use Phera\Log\Logger;
$memory = Logger::memory('boot')->withContext(['phase' => 'boot']);
$memory->info('booted');
$file = new Logger('web', 'info', 'json', __DIR__ . '/web.log', ['app' => 'demo']);
$file->info('booted');
Phera\snapshot();
static $count = 0;
$memory->info('request', ['n' => ++$count]);
$file->info('request {n}', ['n' => $count]);
$file->debug('dropped');
$file->flush();
$records = $memory->records();
echo implode(',', array_column($records, 'message')), ' ', json_encode($records[array_key_last($records)]['context']);
"#,
    )
    .expect("write script");
    let local = tokio::task::LocalSet::new();
    let answers = local
        .run_until(async {
            let worker = Runtime::new().web_worker(0);
            let report = worker.boot(&script, &root).await;
            assert!(
                matches!(report, BootReport::Snapshot { .. }),
                "the boot keeps a snapshot: {}",
                match report {
                    BootReport::Failed(message) => message,
                    _ => "it ended without one".into(),
                }
            );
            let mut answers = Vec::new();
            for _ in 0..3 {
                let response = worker.handle_request(&script, &root, request()).await;
                answers.push(String::from_utf8_lossy(&response.body).into_owned());
            }
            answers
        })
        .await;
    let expected = r#"booted,request {"phase":"boot","n":1}"#;
    assert_eq!(answers, [expected, expected, expected]);
    let log = std::fs::read_to_string(root.join("web.log")).expect("web.log");
    let messages: Vec<_> = log
        .lines()
        .map(|line| {
            let start = line.find(r#""message":""#).expect("a message") + 11;
            &line[start..start + line[start..].find('"').expect("end")]
        })
        .collect();
    assert_eq!(messages, ["booted", "request 1", "request 1", "request 1"]);
    assert!(
        log.lines()
            .all(|line| line.ends_with(r#""context":{"app":"demo","n":1}}"#)
                || line.contains(r#""message":"booted""#)),
        "{log}"
    );
}

//! Regression tests for Breakthrough 11 optimizations:
//! - `CachedSite::StaticCall` (`self::`, `parent::`, `Class::`, `static::` LSB, closures, forks)
//! - Nested `pure_arrays` skip + single-pass `PhpArray::clone_mapped` in snapshot forks
//! - Direct path-tail dispatch in `further_steps.rs`
//! - Generator `run_in_place` + `Releases::pending` `Cell<bool>` finally cleanup

use std::net::SocketAddr;

use phera_runtime::{HttpRequest, Runtime};
use tempfile::tempdir;

fn request(target: &str) -> HttpRequest {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let address: SocketAddr = "127.0.0.1:8080".parse().expect("address");
    HttpRequest {
        method: "GET".to_string(),
        target: target.to_string(),
        path: path.to_string(),
        query: query.to_string(),
        protocol: "HTTP/1.1".to_string(),
        headers: vec![("Host".to_string(), b"example.test".to_vec())],
        body: Vec::new(),
        remote_addr: "203.0.113.9:51000".parse().expect("address"),
        local_addr: address,
        https: false,
    }
}

fn run_script(code: &str) -> String {
    let dir = tempdir().unwrap();
    let script = dir.path().join("test.php");
    std::fs::write(&script, code).unwrap();
    let result = Runtime::new()
        .run_file_blocking(&script, &[], None)
        .expect("run");
    String::from_utf8(result.output).unwrap()
}

fn run_forked(code: &str, requests: usize) -> Vec<String> {
    let dir = tempdir().unwrap();
    let script = dir.path().join("app.php");
    std::fs::write(&script, code).unwrap();
    let runtime = Runtime::new();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let local = tokio::task::LocalSet::new();
    rt.block_on(local.run_until(async {
        let worker = runtime.web_worker(0);
        let _ = worker.boot(&script, dir.path()).await;
        let mut outputs = Vec::with_capacity(requests);
        for index in 0..requests {
            let req = request(&format!("/?i={index}"));
            let res = worker.handle_request(&script, dir.path(), req).await;
            assert_eq!(
                res.status,
                200,
                "request {index} failed: {}",
                String::from_utf8_lossy(&res.body)
            );
            outputs.push(String::from_utf8(res.body).unwrap());
        }
        outputs
    }))
}

#[test]
fn static_call_cache_respects_lsb_closures_and_snapshot_forks() {
    let php = r#"<?php
class Base {
    protected static function tag(): string { return static::class; }
    public static function viaSelf(): string { return self::tag(); }
    public static function viaStatic(): string { return static::who(); }
    public static function who(): string { return 'Base'; }
    protected function inst(): string { return 'inst:' . static::class; }
    public function callSelfInst(): string { return self::inst(); }
}
class Child extends Base {
    public static function who(): string { return 'Child'; }
    public static function viaParent(): string { return parent::tag(); }
    public function callParentInst(): string { return parent::inst(); }
}
class SecretA {
    private static function secret(): string { return 'A'; }
    public static function reader(): Closure { return static fn() => self::secret(); }
}
class SecretB {
    private static function secret(): string { return 'B'; }
}

$base = new Base();
$child = new Child();
$closure = SecretA::reader();
$rebound = $closure->bindTo(null, SecretB::class);

Phera\snapshot();

$out = [];
for ($i = 0; $i < 4; $i++) {
    $out[] = Base::viaSelf();
    $out[] = Child::viaSelf();
    $out[] = Child::viaParent();
    $out[] = Base::viaStatic();
    $out[] = Child::viaStatic();
    $out[] = $base->callSelfInst();
    $out[] = $child->callParentInst();
    $out[] = $closure();
    $out[] = $rebound();
}
echo implode(',', array_slice($out, 0, 9)), '|', count(array_unique(array_chunk($out, 9), SORT_REGULAR));
"#;
    let outputs = run_forked(php, 3);
    for out in outputs {
        assert_eq!(
            out,
            "Base,Child,Child,Base,Child,inst:Base,inst:Child,A,B|1"
        );
    }
}

#[test]
fn nested_pure_and_impure_arrays_stay_isolated_across_forks() {
    let php = r#"<?php
class Counter {
    public int $n = 0;
}
$pure = ['a' => [1, 2, ['deep' => 'ok']], 'b' => ['x' => 'y']];
$holder = new Counter();
$mixed = [
    'pure_nested' => $pure,
    'impure' => ['counter' => $holder, 'pure_sibling' => $pure],
];
Phera\snapshot();
$mixed['impure']['counter']->n += 1;
$mixed['pure_nested']['a'][0] = 99;
echo $mixed['impure']['counter']->n, ':', $mixed['pure_nested']['a'][0], ':', $mixed['impure']['pure_sibling']['a'][0];
"#;
    let outputs = run_forked(php, 4);
    for out in outputs {
        assert_eq!(out, "1:99:1");
    }
}

#[test]
fn path_tail_dispatch_and_generator_releases_behave_identically() {
    let php = r#"<?php
class Box {
    public array $items = [];
    public int $count = 0;
    public static array $staticArr = ['k' => 7];
    public static int $staticCount = 3;
}
$box = new Box();
for ($i = 0; $i < 5; $i++) {
    $box->items[] = $i * 2;
    $box->items['k' . $i] = $i;
    $box->count++;
    $box->items['k' . $i]++;
}
$log = [];
function gen(array &$log) {
    try {
        $v = yield 1;
        $log[] = "got:$v";
        yield 2;
    } finally {
        $log[] = 'closed';
    }
}
$g = gen($log);
$first = $g->current();
$second = $g->send(42);
unset($g);
echo json_encode([
    'count' => $box->count,
    'items' => $box->items,
    'static' => Box::$staticArr['k'] + Box::$staticCount,
    'isset' => [isset($box->count), isset($box->items['k2']), isset(Box::$staticArr['k'])],
    'gen' => [$first, $second, $log],
]);
"#;
    let out = run_script(php);
    assert_eq!(
        out,
        r#"{"count":5,"items":{"0":0,"k0":1,"1":2,"k1":2,"2":4,"k2":3,"3":6,"k3":4,"4":8,"k4":5},"static":10,"isset":[true,true,true],"gen":[1,2,["got:42","closed"]]}"#
    );
}

//! Structure-aware generator: emits PHP files that stress one specific risk
//! at a parameterised depth or size, so the campaigns explore the boundary of
//! each limit instead of random noise.

use crate::rng::Rng;

pub struct GenCase {
    pub name: String,
    pub source: String,
}

fn repeat(s: &str, n: usize) -> String {
    let mut out = String::with_capacity(s.len() * n + 16);
    for _ in 0..n {
        out.push_str(s);
    }
    out
}

/// All generator cases at a given scale. `scale` picks from SCALES.
pub const SCALES: &[usize] = &[8, 64, 500, 999, 1000, 1001, 2000, 5000, 20000, 100000];

pub fn cases(scale: usize, rng: &mut Rng) -> Vec<GenCase> {
    let n = scale;
    let mut v = Vec::new();
    let mut push = |name: &str, source: String| {
        if source.len() <= 8 * 1024 * 1024 {
            v.push(GenCase {
                name: format!("{name}-n{n}"),
                source,
            });
        }
    };

    // --- nesting depth of every nesting construct (parser limit is 1000) ---
    push(
        "paren",
        format!("<?php $x = {}1{};\n", "(".repeat(n), ")".repeat(n)),
    );
    push(
        "array-nest",
        format!("<?php $a = {}[1]{};\n", "[".repeat(n), "]".repeat(n)),
    );
    push(
        "array-nest-assign",
        format!("<?php $a = 1;\nfor ($i = 0; $i < {n}; $i++) {{ $a = [$a]; }}\n"),
    );
    push(
        "ternary",
        format!(
            "<?php $x = {}true ? 1 : 2{};\n",
            repeat("true ? (", n.min(20000)),
            repeat(") : 2", n.min(20000))
        ),
    );
    push(
        "closure-nest",
        format!(
            "<?php $f = {}1{};\n",
            repeat("fn() => ", n.min(20000)),
            repeat("", 0)
        ),
    );
    push(
        "try-nest",
        format!(
            "<?php {}try {{ throw new Exception('x'); }} finally {{ }} {}\n",
            repeat("try { ", n.min(20000)),
            repeat("} catch (Throwable $e) { } finally { } ", n.min(20000))
        ),
    );
    push(
        "match-nest",
        format!(
            "<?php $x = {}1{};\n",
            repeat("match (true) { true => ", n.min(5000)),
            repeat(" }", n.min(5000))
        ),
    );
    push(
        "interp-nest",
        format!(
            "<?php $a = 'x'; $s = \"{}\";\n",
            "{$a".repeat(n.min(20000)) + &"x".to_string() + &"}".repeat(n.min(20000))
        ),
    );
    push(
        "class-chain",
        format!(
            "<?php class C0 {{}}\n{}\n$o = new C{n};\n",
            (1..=n.min(20000))
                .map(|i| format!("class C{i} extends C{} {{}}\n", i - 1))
                .collect::<String>()
        ),
    );
    push(
        "if-nest",
        format!(
            "<?php {}if (true) {{ $x = 1; }} {}\n",
            repeat("if (true) { ", n.min(20000)),
            repeat("}", n.min(20000))
        ),
    );

    // --- long chains (must be loop-built, not recursive) ---
    push("concat-chain", format!("<?php $s = {};\n", repeat("'a' . ", n)));
    push("add-chain", format!("<?php $x = {};\n", repeat("1 + ", n)));
    push("and-chain", format!("<?php $x = {};\n", repeat("true && ", n)));
    push("prop-chain", format!("<?php $o = new stdClass; $o->p = $o; $x = {};\n", repeat("$o->p->p", 1) + &repeat("->p", n)));
    push(
        "method-chain",
        format!(
            "<?php class C {{ public function m() {{ return $this; }} }} $o = new C; $x = $o{};\n",
            repeat("->m()", n.min(20000))
        ),
    );
    push("call-chain", format!("<?php $x = {};\n", repeat("abs(", n.min(20000)) + "1" + &repeat(")", n.min(20000))));
    push("coalesce-chain", format!("<?php $x = {};\n", repeat("$u ?? ", n)));

    // --- strings, identifiers, literals ---
    push("long-string", format!("<?php $s = '{}'; echo strlen($s);\n", repeat("a", n * 64)));
    push("long-ident", format!("<?php ${} = 1;\n", repeat("a", n.min(100000))));
    push("many-statements", format!("<?php {}\n", repeat("$x = 1; ", n)));
    push("many-args-literal", format!("<?php function f(...$a) {{ return count($a); }} echo f({});\n", repeat("1,", n)));
    push("big-array-literal", format!("<?php $a = [{}];\n", repeat("1,", n)));

    // --- recursion in PHP code (VM frames are on the heap) ---
    push(
        "php-recursion",
        format!("<?php function f($n) {{ if ($n <= 0) return 0; return f($n - 1) + 1; }} echo f({});\n", n),
    );
    push(
        "php-mutual-recursion",
        format!("<?php function a($n) {{ return $n <= 0 ? 0 : b($n - 1); }} function b($n) {{ return a($n); }} echo a({});\n", n),
    );
    push(
        "callback-recursion",
        format!(
            "<?php $f = null; $f = function ($n) use (&$f) {{ return $n <= 0 ? 0 : $f($n - 1); }}; echo $f({});\n",
            n
        ),
    );
    push(
        "array-map-recursion",
        format!(
            "<?php $f = function ($x) use (&$f) {{ return is_array($x) ? array_map($f, $x) : $x; }}; $a = 1; for ($i = 0; $i < {}; $i++) {{ $a = [$a]; }} echo $f($a);\n",
            n.min(2000)
        ),
    );
    push(
        "usort-callback",
        format!(
            "<?php $a = range(1, {}); usort($a, function ($x, $y) {{ return $x <=> $y; }}); echo $a[0];\n",
            n.min(100000)
        ),
    );

    // --- deep / cyclic data structures, then freed ---
    push(
        "deep-array-free",
        format!("<?php $a = 1; for ($i = 0; $i < {}; $i++) {{ $a = [$a]; }} unset($a); echo 'done';\n", n.min(200000)),
    );
    push(
        "cycle-free",
        format!(
            "<?php $a = []; $a['self'] = &$a; $b = [$a, $a]; unset($a); unset($b); echo 'done';\n"
        ),
    );
    push(
        "cycle-dump",
        format!("<?php $a = []; $a['self'] = &$a; var_dump($a);\n"),
    );
    push(
        "cycle-serialize",
        format!("<?php $a = []; $a['self'] = &$a; @serialize($a); echo 'done';\n"),
    );
    push(
        "cycle-json",
        format!("<?php $a = []; $a['self'] = &$a; @json_encode($a); echo 'done';\n"),
    );
    push(
        "cycle-compare",
        format!("<?php $a = []; $a['self'] = &$a; $b = []; $b['self'] = &$b; var_dump($a == $b);\n"),
    );
    push(
        "object-cycle",
        format!(
            "<?php class N {{ public $other; }} $a = new N; $b = new N; $a->other = $b; $b->other = $a; unset($a); unset($b); echo 'done';\n"
        ),
    );
    push(
        "deep-array-ops",
        format!(
            "<?php $a = 1; for ($i = 0; $i < {}; $i++) {{ $a = ['k' => $a]; }} echo count($a, COUNT_RECURSIVE);\n",
            n.min(100000)
        ),
    );

    // --- exceptions from awkward places ---
    push(
        "throw-in-args",
        format!(
            "<?php function f($a) {{ return $a; }} try {{ f((function () {{ throw new Exception('x'); }})()); }} catch (Throwable $e) {{ echo 'caught'; }}\n"
        ),
    );
    push(
        "throw-in-finally",
        format!(
            "<?php try {{ try {{ throw new Exception('a'); }} finally {{ throw new Exception('b'); }} }} catch (Throwable $e) {{ echo get_class($e), $e->getMessage(); }}\n"
        ),
    );
    push(
        "throw-in-destruct",
        format!(
            "<?php class D {{ public function __destruct() {{ throw new Exception('d'); }} }} try {{ $d = new D; throw new Exception('x'); }} catch (Throwable $e) {{ echo $e->getMessage(); }}\n"
        ),
    );
    push(
        "throw-in-tostring",
        format!(
            "<?php class S {{ public function __toString(): string {{ throw new Exception('s'); }} }} try {{ echo new S; }} catch (Throwable $e) {{ echo 'caught'; }}\n"
        ),
    );
    push(
        "unwind-deep",
        format!(
            "<?php function f($n) {{ if ($n == 0) {{ throw new Exception('deep'); }} try {{ f($n - 1); }} finally {{ }} }} try {{ f({}); }} catch (Throwable $e) {{ echo 'caught'; }}\n",
            n.min(50000)
        ),
    );
    push(
        "throw-in-generator",
        format!(
            "<?php function g() {{ try {{ yield 1; throw new Exception('g'); }} finally {{ echo 'fin'; }} }} $gen = g(); $gen->current(); try {{ $gen->next(); }} catch (Throwable $e) {{ echo 'caught'; }}\n"
        ),
    );

    // --- hostile data to decoders / matchers ---
    push(
        "json-deep",
        format!("<?php $s = '{}1{}'; json_decode($s); echo 'done';\n", "[".repeat(n), "]".repeat(n)),
    );
    push(
        "json-deep-obj",
        format!("<?php $s = '{}\"a\":1{}'; json_decode($s); echo 'done';\n", "{\"a\":".repeat(n), "}".repeat(n)),
    );
    push(
        "json-big-num",
        format!("<?php json_decode('1e{}'); echo 'done';\n", n),
    );
    push(
        "json-bad-utf8",
        format!("<?php json_decode(\"{}\"); echo 'done';\n", repeat("\\u00e9\\ud800", n.min(1000))),
    );
    push(
        "preg-catastrophic",
        format!(
            "<?php preg_match('/(a+)+$/', '{}b'); echo 'done';\n",
            repeat("a", n.min(40))
        ),
    );
    push(
        "preg-catastrophic2",
        format!(
            "<?php preg_match('/(a|a)*$/', '{}b'); echo 'done';\n",
            repeat("a", n.min(40))
        ),
    );
    push(
        "preg-nested-quant",
        format!(
            "<?php preg_match('/{}b/', '{}b'); echo 'done';\n",
            repeat("(a*)", n.min(30)),
            repeat("a", n.min(40))
        ),
    );
    push(
        "preg-bad-utf8-u",
        format!("<?php @preg_match('/./u', \"{}\"); echo 'done';\n", repeat("\u{fffd}", n.min(100))),
    );
    push(
        "preg-huge-subject",
        format!("<?php preg_match('/a/', '{}'); echo 'done';\n", repeat("a", n.min(200000))),
    );
    push(
        "preg-replace-catastrophic",
        format!(
            "<?php preg_replace('/(a+)+$/', 'x', '{}b'); echo 'done';\n",
            repeat("a", n.min(40))
        ),
    );
    push(
        "sprintf-repeat-fmt",
        format!("<?php @sprintf('{}', 1); echo 'done';\n", repeat("%s", n.min(100000))),
    );
    push(
        "sprintf-huge-width",
        format!("<?php @sprintf('%{}d', 1); echo 'done';\n", n),
    );
    push(
        "sprintf-positional",
        format!("<?php @sprintf('{}', 1, 2, 3); echo 'done';\n", repeat("%1$s ", n.min(1000))),
    );
    push("str-repeat-huge", format!("<?php @str_repeat('ab', {}); echo 'done';\n", n * 1000));
    push("str-repeat-neg", format!("<?php @str_repeat('ab', -{}); echo 'done';\n", n));
    push("str-pad-huge", format!("<?php @str_pad('a', {}); echo 'done';\n", n * 1000));
    push("range-huge", format!("<?php @range(1, {}); echo 'done';\n", n * 1000));
    push("range-float-step", format!("<?php @range(1, 2, {}); echo 'done';\n", 1.0 / n.max(1) as f64));
    push("array-fill-huge", format!("<?php @array_fill(0, {}, 'x'); echo 'done';\n", n * 1000));
    push("array-chunk-huge", format!("<?php @array_chunk(range(1, 100), {}); echo 'done';\n", n));
    push("str-split-huge", format!("<?php @str_split('abc', {}); echo 'done';\n", n));
    push("explode-huge", format!("<?php @explode(',', '{}'); echo 'done';\n", repeat(",", n.min(100000))));

    // --- integer / float boundary behaviour ---
    push("int-overflow-add", "<?php $x = PHP_INT_MAX; var_dump($x + 1, $x + $x, PHP_INT_MIN - 1, ~PHP_INT_MAX, 2 ** 63, 2 ** 64, 9223372036854775807 * 2);\n".into());
    push("float-precision", "<?php var_dump(0.1 + 0.2, 1e308 * 10, -1e308 * 10, INF - INF, NAN == NAN, fdiv(1, 0), intdiv(PHP_INT_MIN, -1));\n".into());
    push("string-offset", "<?php $s = 'abc'; var_dump($s[PHP_INT_MAX], $s[-PHP_INT_MAX], $s[9223372036854775807], $s['1'], $s[1.9], $s[true], $s[null]);\n".into());
    push("string-num-compare", "<?php var_dump('1e3' == 1000, '0x10' == 16, ' 1' == 1, '1 ' == 1, '01' == 1, '1.0' == 1, 'abc' == 0, '' == 0, '0' == false, '' == false, [] == false, [0] == false);\n".into());
    push("array-key-normalise", "<?php $a = []; $a['9223372036854775807'] = 1; $a['9223372036854775808'] = 2; $a['-9223372036854775808'] = 3; $a['1.5'] = 4; $a['01'] = 5; $a[' 1'] = 6; $a[true] = 7; $a[null] = 8; $a[1.9] = 9; var_dump($a);\n".into());
    push("sort-big", format!("<?php $a = range(1, {}); shuffle($a); sort($a); rsort($a); usort($a, fn($x, $y) => $x <=> $y); echo $a[0];\n", n.min(200000)));
    push("array-unique-big", format!("<?php $a = array_fill(0, {}, 'x'); var_dump(count(array_unique($a)));\n", n.min(100000)));
    push("hash-collide", format!("<?php $a = []; for ($i = 0; $i < {}; $i++) {{ $a['k' . $i] = $i; }} echo count($a);\n", n.min(200000)));

    // --- visibility / readonly / strict_types behaviour probes ---
    push(
        "private-access",
        "<?php class A { private $x = 1; } class B extends A { function f() { return $this->x; } } try { echo (new B)->f(); } catch (Throwable $e) { echo get_class($e); }\n".into(),
    );
    push(
        "protected-access",
        "<?php class A { protected $x = 1; } class B extends A { function f() { return $this->x; } } echo (new B)->f(); class C { function g() { return (new A)->x; } } try { echo (new C)->g(); } catch (Throwable $e) { echo get_class($e); }\n".into(),
    );
    push(
        "readonly-write",
        "<?php class R { public function __construct(public readonly int $x) {} } $r = new R(1); try { $r->x = 2; } catch (Throwable $e) { echo get_class($e); }\n".into(),
    );
    push(
        "strict-types",
        "<?php declare(strict_types=1); function f(int $x): int { return $x; } try { var_dump(f('1')); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(); } try { var_dump(f(1.5)); } catch (Throwable $e) { echo get_class($e); }\n".into(),
    );
    push(
        "serialize-visibility",
        "<?php class A { private $p = 1; protected $q = 2; public $r = 3; } echo serialize(new A), \"\\n\";\n".into(),
    );

    // a little randomness: pick a random subset ordering so campaigns differ
    if rng.chance(1, 2) {
        v.reverse();
    }
    v
}

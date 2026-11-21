--TEST--
array_map callbacks on the frame stack: keys, nesting, closures, traces, destructors, errors
--FILE--
<?php
declare(strict_types=1);
class Noisy { public function __construct(public string $n) {} public function __destruct() { echo "destruct {$this->n}\n"; } }
class Holder {
    private int $base = 10;
    public function add(array $values): array { return array_map(fn ($v) => $v + $this->base, $values); }
}
function gen($v) { yield $v; }
function run(): void
{
    for ($round = 0; $round < 2; $round++) {
        echo json_encode(array_map(fn ($v) => $v * 2, [3 => 1, 'a' => 2, 'b' => 3])), "\n";
        echo json_encode(array_map(fn ($v) => $v, [])), "\n";
        $total = 0;
        echo json_encode(array_map(function ($v) use (&$total) { $total += $v; return $total; }, [1, 2, 3])), " $total\n";
        echo json_encode(array_map(fn ($row) => array_map(fn ($v) => $v + 1, $row), [[1, 2], [3]])), "\n";
        echo json_encode((new Holder)->add([1, 2])), "\n";
        echo json_encode(array_map(function ($v) { $local = new Noisy("in $v"); return $v; }, [1, 2])), "\n";
        echo get_class(array_map('gen', [1])[0]), "\n";
        echo json_encode(array_map('strtoupper', ['a', 'b'])), "\n";
        echo json_encode(array_map(fn ($a, $b) => $a . $b, ['x', 'y'], [1, 2])), "\n";
        echo json_encode(array_map(fn (int $v): int => $v * 3, [1, 2])), "\n";
        try {
            array_map(function ($v) { if ($v === 2) { throw new RuntimeException("at $v"); } return $v; }, [1, 2, 3]);
        } catch (RuntimeException $e) {
            echo $e->getMessage(), "\n", preg_replace('/\(\d+\)/', '(N)', $e->getTraceAsString()), "\n";
        }
        try {
            array_map(fn (int $v) => $v, ['x']);
        } catch (TypeError $e) {
            echo get_class($e), ': ', $e->getMessage(), "\n";
        }
        $kept = array_map(fn ($v) => new Noisy("kept $v"), [1]);
        echo "before unset\n";
        unset($kept);
        echo "end of round\n";
    }
}
run();
--EXPECTF--
{"3":2,"a":4,"b":6}
[]
[1,3,6] 6
[[2,3],[4]]
[11,12]
destruct in 1
destruct in 2
[1,2]
Generator
["A","B"]
["x1","y2"]
[3,6]
at 2
#0 [internal function]: {closure:run():24}(N)
#1 %s(N): array_map(Object(Closure), Array)
#2 %s(N): run()
#3 {main}
TypeError: {closure:run():29}(): Argument #1 ($v) must be of type int, string given
before unset
destruct kept 1
end of round
{"3":2,"a":4,"b":6}
[]
[1,3,6] 6
[[2,3],[4]]
[11,12]
destruct in 1
destruct in 2
[1,2]
Generator
["A","B"]
["x1","y2"]
[3,6]
at 2
#0 [internal function]: {closure:run():24}(N)
#1 %s(N): array_map(Object(Closure), Array)
#2 %s(N): run()
#3 {main}
TypeError: {closure:run():29}(): Argument #1 ($v) must be of type int, string given
before unset
destruct kept 1
end of round

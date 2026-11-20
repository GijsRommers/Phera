--TEST--
Returns from callbacks to native callers give the same results as the general path
--FILE--
<?php
class Point
{
    public function __construct(public int $x = 0) { echo "construct {$x}\n"; }
}
class Noisy
{
    public function __construct(public string $name) {}
    public function __destruct() { echo "destruct {$this->name}\n"; }
}
function &refs(array &$store) { return $store[0]; }
function floaty(int $value): float { return $value; }
function stringy($value): string { return $value * 2; }
function nothing($value): void { }
function gen() { yield 1; yield 2; return 3; }

function run(): void
{
    for ($round = 0; $round < 2; $round++) {
        echo json_encode(array_map(fn ($v) => $v * 2, [1, 2, 3])), "\n";
        echo json_encode(array_map('floaty', [1, 2])), "\n";
        echo json_encode(array_map('stringy', [1, 2])), "\n";
        echo json_encode(array_map('nothing', [1])), "\n";
        $list = [3, 1, 2];
        usort($list, function ($a, $b) { return $a <=> $b; });
        echo json_encode($list), "\n";
        echo json_encode(array_filter([1, 0, 2, null], fn ($v) => (bool) $v)), "\n";
        $points = array_map(fn ($x) => new Point($x), [1, 2]);
        echo count($points), "\n";
        $made = (new ReflectionClass(Point::class))->newInstance(7);
        echo $made->x, "\n";
        echo call_user_func(function () { $local = new Noisy('local'); return 'returned'; }), "\n";
        echo call_user_func(fn () => new Noisy('kept'))->name, "\n";
        echo implode(',', iterator_to_array(gen())), "\n";
        try {
            array_map(function ($v) { if ($v > 1) { throw new RuntimeException("at $v"); } return $v; }, [1, 2]);
        } catch (RuntimeException $e) {
            echo $e->getMessage(), "\n";
        }
        try {
            array_map(fn ($v): int => 'nope', [1]);
        } catch (TypeError $e) {
            echo $e->getMessage(), "\n";
        }
        $store = [1];
        $value = call_user_func_array('refs', [&$store]);
        echo $value, "\n";
        echo array_reduce([1, 2, 3], fn ($carry, $v) => $carry + $v, 10), "\n";
    }
}
run();
--EXPECT--
[2,4,6]
[1,2]
["2","4"]
[null]
[1,2,3]
{"0":1,"2":2}
construct 1
construct 2
2
construct 7
7
destruct local
returned
destruct kept
kept
1,2
at 2
{closure:run():41}(): Return value must be of type int, string returned
1
16
[2,4,6]
[1,2]
["2","4"]
[null]
[1,2,3]
{"0":1,"2":2}
construct 1
construct 2
2
construct 7
7
destruct local
returned
destruct kept
kept
1,2
at 2
{closure:run():41}(): Return value must be of type int, string returned
1
16

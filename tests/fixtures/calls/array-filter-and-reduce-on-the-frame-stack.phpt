--TEST--
array_filter and array_reduce callbacks on the frame stack: modes, keys, nesting, NAN, traces
--FILE--
<?php
class Item { public function __construct(public int $n, public bool $on) {} }
function run(): void
{
    $items = ['a' => new Item(1, true), 'b' => new Item(2, false), 7 => new Item(3, true)];
    for ($round = 0; $round < 2; $round++) {
        echo json_encode(array_keys(array_filter($items, fn ($item) => $item->on))), "\n";
        echo json_encode(array_keys(array_filter($items, fn ($key) => is_string($key), ARRAY_FILTER_USE_KEY))), "\n";
        echo json_encode(array_keys(array_filter($items, fn ($item, $key) => $item->n > 1 && $key !== 7, ARRAY_FILTER_USE_BOTH))), "\n";
        echo json_encode(array_filter([1, 0, 2, null, '', 'x'], fn ($v) => $v)), "\n";
        echo json_encode(array_filter([1, 0, 2])), "\n";
        echo json_encode(array_filter([], fn ($v) => true)), "\n";
        echo array_reduce($items, fn ($sum, $item) => $sum + $item->n, 10), "\n";
        echo var_export(array_reduce([], fn ($c, $v) => $c + $v), true), "\n";
        echo array_reduce([1, 2, 3], fn ($carry, $v) => $carry . $v, 'x'), "\n";
        echo json_encode(array_filter([[1, 2], [3]], fn ($row) => count(array_filter($row, fn ($v) => $v > 1)) > 0)), "\n";
        echo json_encode(array_filter([1.5, 2.5], fn ($v) => NAN)), "\n";
        echo json_encode(array_filter(['a', 'B'], 'ctype_upper')), "\n";
        try {
            array_reduce([1, 2], function ($c, $v) { if ($v === 2) { throw new LogicException('stop'); } return $v; });
        } catch (LogicException $e) {
            echo preg_replace('/\(\d+\)/', '(N)', $e->getTraceAsString()), "\n";
        }
    }
}
run();
--EXPECTF--
["a",7]
["a","b"]
["b"]
{"0":1,"2":2,"5":"x"}
{"0":1,"2":2}
[]
16
NULL
x123
[[1,2],[3]]

Warning: unexpected NAN value was coerced to bool in %s on line 17

Warning: unexpected NAN value was coerced to bool in %s on line 17
[1.5,2.5]
{"1":"B"}
#0 [internal function]: {closure:run():20}(1, 2)
#1 %s(N): array_reduce(Array, Object(Closure))
#2 %s(N): run()
#3 {main}
["a",7]
["a","b"]
["b"]
{"0":1,"2":2,"5":"x"}
{"0":1,"2":2}
[]
16
NULL
x123
[[1,2],[3]]

Warning: unexpected NAN value was coerced to bool in %s on line 17

Warning: unexpected NAN value was coerced to bool in %s on line 17
[1.5,2.5]
{"1":"B"}
#0 [internal function]: {closure:run():20}(1, 2)
#1 %s(N): array_reduce(Array, Object(Closure))
#2 %s(N): run()
#3 {main}

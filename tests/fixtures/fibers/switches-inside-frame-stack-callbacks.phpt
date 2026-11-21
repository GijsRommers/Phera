--TEST--
Fibers suspended, resumed and thrown into inside array_map, array_filter, array_reduce and usort callbacks, and generators inside them
--FILE--
<?php
// A fiber suspended inside the callbacks of built-ins, resumed from the top level.
$builtins = [
    'array_map' => fn (callable $f) => array_map($f, [1, 2, 3]),
    'array_filter' => fn (callable $f) => array_values(array_filter([1, 2, 3], $f)),
    'array_reduce' => fn (callable $f) => array_reduce([1, 2, 3], fn ($carry, $x) => $carry + $f($x), 0),
    'usort' => function (callable $f) { $a = [3, 1, 2]; usort($a, fn ($x, $y) => $f($x) <=> $f($y)); return $a; },
];
foreach ($builtins as $name => $run) {
    echo "== $name\n";
    $fiber = new Fiber(function () use ($run) {
        return $run(function ($x) { $back = Fiber::suspend("at $x"); return $x * 10 + $back; });
    });
    $seen = [$fiber->start()];
    $n = 0;
    while (!$fiber->isTerminated()) {
        $value = $fiber->resume(++$n);
        if ($value !== null) $seen[] = $value;
    }
    echo implode(', ', $seen), "\n";
    echo json_encode($fiber->getReturn()), "\n";
}

// A resume issued from inside each built-in's callbacks.
foreach ($builtins as $name => $run) {
    echo "== resume inside $name\n";
    $fiber = new Fiber(function () { $total = 0; while (true) { $x = Fiber::suspend($total); if ($x === null) return $total; $total += $x; } });
    $fiber->start();
    $result = $run(function ($x) use ($fiber) { return $fiber->resume($x) + 0 * $x + $x; });
    echo json_encode($result), "\n";
    $fiber->resume(null);
    echo "total ", $fiber->getReturn(), "\n";
}

// Fiber::throw into a suspended callback.
foreach ($builtins as $name => $run) {
    echo "== throw into $name\n";
    $fiber = new Fiber(function () use ($run) {
        try {
            return $run(function ($x) { return Fiber::suspend($x); });
        } catch (RuntimeException $e) {
            return "caught " . $e->getMessage();
        }
    });
    $fiber->start();
    $fiber->throw(new RuntimeException("from outside"));
    echo $fiber->getReturn(), "\n";
}

// Generators inside the callbacks, and a fiber suspended inside a generator in a callback.
function numbers() { yield 1; yield 2; }
echo "== generators\n";
echo json_encode(array_map(fn ($x) => iterator_to_array(numbers()), [1, 2])), "\n";
$fiber = new Fiber(function () {
    return array_map(function ($x) {
        $g = (function () use ($x) { $got = Fiber::suspend("gen $x"); yield $x + $got; })();
        return iterator_to_array($g);
    }, [1, 2]);
});
echo $fiber->start(), "\n";
echo $fiber->resume(100), "\n";
$fiber->resume(200);
echo json_encode($fiber->getReturn()), "\n";

--EXPECT--
== array_map
at 1, at 2, at 3
[11,22,33]
== array_filter
at 1, at 2, at 3
[1,2,3]
== array_reduce
at 1, at 2, at 3
66
== usort
at 3, at 1, at 2, at 1, at 3, at 2
[1,2,3]
== resume inside array_map
[2,5,9]
total 6
== resume inside array_filter
[1,2,3]
total 6
== resume inside array_reduce
16
total 6
== resume inside usort
[1,3,2]
total 12
== throw into array_map
caught from outside
== throw into array_filter
caught from outside
== throw into array_reduce
caught from outside
== throw into usort
caught from outside
== generators
[[1,2],[1,2]]
gen 1
gen 2
[[101],[202]]

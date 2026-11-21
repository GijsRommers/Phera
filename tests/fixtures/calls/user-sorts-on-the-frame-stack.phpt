--TEST--
usort, uasort and uksort with comparisons on the frame stack: order, ties, bool results, exceptions, nesting
--FILE--
<?php
$a = [5, 3, 9, 1, 7, 3, 8, 2, 6, 4, 0, 11, 15, 13, 12, 14, 10, 19, 18, 17, 16];
$calls = 0;
var_dump(usort($a, function ($x, $y) use (&$calls) { $calls++; return $x <=> $y; }));
echo implode(',', $a), " calls=$calls\n";

$pairs = [];
$b = [3, 1, 2];
usort($b, function ($x, $y) use (&$pairs) { $pairs[] = "$x:$y"; return $x <=> $y; });
echo implode(',', $b), ' ', implode(' ', $pairs), "\n";

// Ties keep the original order.
$people = [['n' => 'a', 'g' => 2], ['n' => 'b', 'g' => 1], ['n' => 'c', 'g' => 2], ['n' => 'd', 'g' => 1]];
usort($people, fn ($p, $q) => $p['g'] <=> $q['g']);
echo implode(',', array_column($people, 'n')), "\n";

// uasort keeps keys, uksort compares keys.
$m = ['x' => 3, 'y' => 1, 'z' => 2];
uasort($m, fn ($p, $q) => $p <=> $q);
var_dump($m);
$k = ['b' => 1, 'c' => 2, 'a' => 3];
uksort($k, fn ($p, $q) => strcmp($p, $q));
var_dump($k);

// A bool result is deprecated; false asks again swapped.
$asked = [];
$c = [2, 3, 1];
usort($c, function ($x, $y) use (&$asked) { $asked[] = "$x:$y"; return $x > $y; });
echo implode(',', $c), ' ', implode(' ', $asked), "\n";

// An exception from the callback: the sort finishes, the copy is stored.
$d = [4, 2, 3, 1];
$n = 0;
try {
    usort($d, function ($x, $y) use (&$n) {
        if (++$n === 2) {
            throw new RuntimeException("stop at $x:$y");
        }
        return $x <=> $y;
    });
} catch (RuntimeException $e) {
    echo $e->getMessage(), ' ', implode(',', $d), "\n";
}

// A sort inside a comparison.
$outer = [[3, 1], [2, 9, 4], [1]];
usort($outer, function ($x, $y) {
    usort($x, fn ($p, $q) => $p <=> $q);
    usort($y, fn ($p, $q) => $p <=> $q);
    return $x[0] <=> $y[0];
});
echo json_encode($outer), "\n";

// One element and none.
$one = [7];
var_dump(usort($one, fn ($x, $y) => throw new LogicException('never')));
$none = [];
var_dump(usort($none, fn ($x, $y) => 0));

// The callback changes the array; the sorted copy wins.
$e = [3, 1, 2];
usort($e, function ($x, $y) use (&$e) { $e = 'gone'; return $x <=> $y; });
echo implode(',', $e), "\n";

// A result that is not an integer.
$f = [3, 1, 2];
usort($f, fn ($x, $y) => (string) ($x - $y));
echo implode(',', $f), "\n";
$g = [1.5, 1.2, 1.9];
usort($g, fn ($x, $y) => $x - $y);
echo implode(',', $g), "\n";

// A typed parameter the element does not fit.
try {
    $h = [1, 'x', 2];
    usort($h, fn (int $x, int $y) => $x <=> $y);
} catch (TypeError $e) {
    echo get_class($e), "\n";
}

// The trace names usort.
try {
    $t = [2, 1];
    usort($t, fn ($x, $y) => throw new Exception('t'));
} catch (Exception $e) {
    echo $e->getTrace()[1]['function'] ?? '-', "\n";
}

// Big enough for the quicksort part.
$big = [];
for ($i = 0; $i < 200; $i++) { $big[] = ($i * 7919) % 211; }
$count = 0;
usort($big, function ($x, $y) use (&$count) { $count++; return $x <=> $y; });
echo md5(implode(',', $big)), " $count\n";

--EXPECTF--
bool(true)
0,1,2,3,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19 calls=60
1,2,3 3:1 2:1 3:2
b,d,a,c
array(3) {
  ["y"]=>
  int(1)
  ["z"]=>
  int(2)
  ["x"]=>
  int(3)
}
array(3) {
  ["a"]=>
  int(3)
  ["b"]=>
  int(1)
  ["c"]=>
  int(2)
}

Deprecated: usort(): Returning bool from comparison function is deprecated, return an integer less than, equal to, or greater than zero in %s on line 28
1,2,3 2:3 3:2 3:1 2:1
stop at 3:2 2,4,3,1
[[3,1],[1],[2,9,4]]
bool(true)
bool(true)
1,2,3
1,2,3
1.5,1.2,1.9
TypeError
usort
8be7c3b0d48b13c1c63945178f4c88c2 1514

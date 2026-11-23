--TEST--
=== and !== of objects, arrays, enums, closures and references
--FILE--
<?php
class A {}
function same($x, $y) { return [$x === $y, $x !== $y]; }
$a = new A; $b = new A; $c = $a;
$arr = [1, 'a' => [2, 3], 'o' => $a];
$copy = $arr; $other = [1, 'a' => [2, 3], 'o' => $b];
$cases = [
  [$a, $c], [$a, $b], [$a, null], [null, null], [$arr, $copy], [$arr, $other],
  [[1, 2], [1, 2]], [[1, 2], [2 => 1]], [['1' => 1], [1 => 1]], [[1, 2], ['0' => 1, '1' => 2]],
  [[1.0], [1]], [[], []], [$arr, $a], ['1', 1], [[NAN], [NAN]], [fn() => 1, fn() => 1],
];
foreach ($cases as $i => [$x, $y]) { echo $i, ':', json_encode(same($x, $y)), "\n"; }
enum E { case X; case Y; }
var_dump(E::X === E::X, E::X === E::Y, E::X !== E::Y);
$r = 5; $s = &$r; var_dump($s === 5);
$deep = []; $p = &$deep; for ($i = 0; $i < 3; $i++) { $p['n'] = []; $p = &$p['n']; } unset($p);
var_dump($deep === $deep, $deep === ['n' => ['n' => ['n' => []]]]);

--EXPECTF--
0:[true,false]
1:[false,true]
2:[false,true]
3:[true,false]
4:[true,false]
5:[false,true]
6:[true,false]
7:[false,true]
8:[true,false]
9:[true,false]
10:[false,true]
11:[true,false]
12:[false,true]
13:[false,true]
14:[false,true]
15:[false,true]
bool(true)
bool(false)
bool(true)
bool(true)
bool(true)
bool(true)

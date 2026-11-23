--TEST--
foreach_into_locals_and_continue
--FILE--
<?php
function sum($arr) {
    $sum = 0;
    foreach ($arr as $k => $v) { $sum += $v + $k; }
    foreach ($arr as $v2) { $sum += $v2; }
    foreach ($arr as $k2 => $v3) { if ($v3 > 1) continue; $sum += $k2; }
    foreach ($arr as $same => $same) { $sum += 1; }
    return $sum;
}
var_dump(sum([1, 2, 3]));
$a = [1, 2, 3]; $r = 0;
foreach ($a as &$r) { $r += 10; }
var_dump($r, $a);
$x = 5; $ref = &$x;
foreach ([7, 8] as $ref) { }
var_dump($x, $ref);
$y = 1; $z = &$y;
foreach ([10, 20] as $y) { echo "iter $y\n"; }
var_dump($y, $z);
$obj = new stdClass();
foreach ([['a', 'b']] as $obj->p) { var_dump($obj->p); }
foreach ([[1, 2], [3, 4]] as [$p, $q]) { echo "$p-$q\n"; }
$outer = [[1, 2], [3]];
foreach ($outer as $inner) { foreach ($inner as $n) { echo "$n,"; } }
echo "\n";
class C implements IteratorAggregate { public function getIterator(): Iterator { return new ArrayIterator([5, 6]); } }
foreach (new C() as $k => $v) { echo "$k=$v "; }
echo "\n";
$gen = (function () { yield 1 => 'a'; yield 2 => 'b'; })();
foreach ($gen as $k => $v) { echo "$k=$v "; }
echo "\n";
foreach ([1, 2] as $i) { foreach ([3, 4] as $j) { if ($j == 4) continue 2; echo "$i$j "; } }
echo "\n";
// continue guarded by a condition in while/do-while/for
$w = 0; $i = 0;
while ($i < 5) { $i++; if ($i % 2 == 0) continue; $w += $i; }
var_dump($w);
$d = 0; $j = 0;
do { $j++; if ($j < 3) continue; $d += $j; } while ($j < 5);
var_dump($d);
$f = 0;
for ($m = 0; $m < 5; $m++) { if ($m == 2) continue; $f += $m; }
var_dump($f);
// continue out of a try must still run finally
function withFinally($arr) {
    $out = '';
    foreach ($arr as $v) {
        try { if ($v == 2) continue; $out .= "t$v"; }
        finally { $out .= "f$v"; }
    }
    return $out;
}
var_dump(withFinally([1, 2, 3]));
--EXPECTF--
int(18)
int(13)
array(3) {
  [0]=>
  int(11)
  [1]=>
  int(12)
  [2]=>
  &int(13)
}
int(8)
int(8)
iter 10
iter 20
int(20)
int(20)
array(2) {
  [0]=>
  string(1) "a"
  [1]=>
  string(1) "b"
}
1-2
3-4
1,2,3,
0=5 1=6 
1=a 2=b 
13 23 
int(9)
int(12)
int(8)
string(10) "t1f1f2t3f3"

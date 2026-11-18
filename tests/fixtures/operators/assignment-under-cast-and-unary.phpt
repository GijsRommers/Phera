--TEST--
An assignment after a cast or a unary operator belongs to the variable, not to the operator
--FILE--
<?php
function f() { return "5"; }
if (!$a = f()) { echo "no\n"; } var_dump($a);
$r = (bool) $b = f(); var_dump($r, $b);
$r = (int) $c = "7x" ?? 1; var_dump($r, $c);
$r = -$d = 3; var_dump($r, $d);
$r = @$e = f(); var_dump($r, $e);
$r = (array) $g = 1; var_dump($r, $g);
$r = ~$h = 1; var_dump($r, $h);
$r = 2 + $i = 3; var_dump($r, $i);
$r = "a" . $j = "b"; var_dump($r, $j);
$k = 1; $r = 1 + $k += 2 ?? 0; var_dump($r);
$arr = [];
$r = (string) $arr['x'] = 5; var_dump($r, $arr);
$r = !$l ??= 0; var_dump($r, $l);
$m = "3"; $r = (int) $m .= "4"; var_dump($r, $m);
$r = true && $n = 5; var_dump($r, $n);
$r = clone $o = new stdClass; var_dump($r == $o);
$r = (float) [$p, $q] = [1, 2]; var_dump($r, $p, $q);
--EXPECT--
string(1) "5"
bool(true)
string(1) "5"
int(7)
string(2) "7x"
int(-3)
int(3)
string(1) "5"
string(1) "5"
array(1) {
  [0]=>
  int(1)
}
int(1)
int(-2)
int(1)
int(5)
int(3)
string(2) "ab"
string(1) "b"
int(4)
string(1) "5"
array(1) {
  ["x"]=>
  int(5)
}
bool(true)
int(0)
int(34)
string(2) "34"
bool(true)
int(5)
bool(true)
float(1)
int(1)
int(2)

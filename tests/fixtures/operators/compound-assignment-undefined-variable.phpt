--TEST--
A compound assignment to a variable that was never assigned warns, as reading it does
--FILE--
<?php
$a += 2; var_dump($a);
$b .= "x"; var_dump($b);
$r = 1 + $c += 2; var_dump($r, $c);
$r = -$d -= 2; var_dump($r, $d);
$r = !$e .= "x"; var_dump($r, $e);
function f() { $x += 1; $r = 1 + $y *= 2; return [$x, $y, $r]; }
var_dump(f());
function both() { $left += $right; $s .= $t; return [$left, $s]; }
var_dump(both());
function ops() { $a -= 1; $b *= 2; $c /= 2; $d %= 2; $e **= 2; $f |= 1; $g &= 1; $h ^= 1; $i <<= 1; $j >>= 1; $k ??= 5; return compact('a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k'); }
var_dump(ops());
function twice() { $n += 1; $n += 1; return $n; } var_dump(twice());
function arr() { $list[] = 1; $list['k'] .= 'x'; $m['a']['b'] += 1; return [$list, $m]; } var_dump(arr());
function refd() { $r = &$q; $q += 1; return $r; } var_dump(refd());
function inloop() { for ($i = 0; $i < 3; $i++) { $sum += $i; } return $sum; } var_dump(inloop());
--EXPECTF--

Warning: Undefined variable $a in %s on line 2
int(2)

Warning: Undefined variable $b in %s on line 3
string(1) "x"

Warning: Undefined variable $c in %s on line 4
int(3)
int(2)

Warning: Undefined variable $d in %s on line 5
int(2)
int(-2)

Warning: Undefined variable $e in %s on line 6
bool(false)
string(1) "x"

Warning: Undefined variable $x in %s on line 7

Warning: Undefined variable $y in %s on line 7
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(0)
  [2]=>
  int(1)
}

Warning: Undefined variable $right in %s on line 9

Warning: Undefined variable $left in %s on line 9

Warning: Undefined variable $t in %s on line 9

Warning: Undefined variable $s in %s on line 9
array(2) {
  [0]=>
  int(0)
  [1]=>
  string(0) ""
}

Warning: Undefined variable $a in %s on line 11

Warning: Undefined variable $b in %s on line 11

Warning: Undefined variable $c in %s on line 11

Warning: Undefined variable $d in %s on line 11

Warning: Undefined variable $e in %s on line 11

Warning: Undefined variable $f in %s on line 11

Warning: Undefined variable $g in %s on line 11

Warning: Undefined variable $h in %s on line 11

Warning: Undefined variable $i in %s on line 11

Warning: Undefined variable $j in %s on line 11
array(11) {
  ["a"]=>
  int(-1)
  ["b"]=>
  int(0)
  ["c"]=>
  int(0)
  ["d"]=>
  int(0)
  ["e"]=>
  int(0)
  ["f"]=>
  int(1)
  ["g"]=>
  int(0)
  ["h"]=>
  int(1)
  ["i"]=>
  int(0)
  ["j"]=>
  int(0)
  ["k"]=>
  int(5)
}

Warning: Undefined variable $n in %s on line 13
int(2)

Warning: Undefined array key "k" in %s on line 14

Warning: Undefined variable $m in %s on line 14

Warning: Undefined array key "a" in %s on line 14

Warning: Undefined array key "b" in %s on line 14
array(2) {
  [0]=>
  array(2) {
    [0]=>
    int(1)
    ["k"]=>
    string(1) "x"
  }
  [1]=>
  array(1) {
    ["a"]=>
    array(1) {
      ["b"]=>
      int(1)
    }
  }
}
int(1)

Warning: Undefined variable $sum in %s on line 16
int(3)

--TEST--
Compound writes reread compiled keys after the right side and delayed reads retain PHP source lines
--FILE--
<?php
function change(&$x) {
    $x=1;
    return 5;
}
set_error_handler(function ($n, $m, $file, $line) {
    echo "$line: $m\n"; return true;
});
$x = [1,2]; $i = 0;
var_dump($x[$i] += change($i), $x);
$x = [1,2]; $i = 0;
var_dump($x[$i] ??= change($i), $x);
$x = [null,null]; $i = 0;
var_dump($x[$i] ??= change($i), $x);
var_dump(
    $undefL
    + change($rhs)
);
var_dump([
    $undefKey =>
    change($rhs)
]);
var_dump(match ($undefMatch) {
    change($rhs) => 'changed',
    null => 'null',
});
--EXPECT--
int(7)
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(7)
}
int(1)
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(2)
}
int(5)
array(2) {
  [0]=>
  NULL
  [1]=>
  int(5)
}
17: Undefined variable $undefL
int(5)
21: Undefined variable $undefKey
array(1) {
  [""]=>
  int(5)
}
24: Undefined variable $undefMatch
25: Undefined variable $undefMatch
string(4) "null"

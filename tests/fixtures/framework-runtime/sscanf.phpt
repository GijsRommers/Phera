--TEST--
Scan numbers, byte strings, ranges and references
--FILE--
<?php
var_dump(sscanf('42 077 077 2a 2A 3 -2e3 80.1 word xy az %', '%D %i %o %x %X %u %E %g %s %2c %[a-z] %%'));
$first = $second = null;
var_dump(sscanf('42 token', '%2$d %1$s', $first, $second), $first, $second);
var_dump(sscanf('xyz', '%d %s'), sscanf('', '%d %s'));
var_dump(sscanf('xx 7', '%*s %d%n'));
--EXPECT--
array(11) {
  [0]=>
  int(42)
  [1]=>
  int(63)
  [2]=>
  int(63)
  [3]=>
  int(42)
  [4]=>
  int(42)
  [5]=>
  int(3)
  [6]=>
  float(-2000)
  [7]=>
  float(80.1)
  [8]=>
  string(4) "word"
  [9]=>
  string(2) "xy"
  [10]=>
  string(2) "az"
}
int(2)
string(5) "token"
int(42)
array(2) {
  [0]=>
  NULL
  [1]=>
  NULL
}
NULL
array(2) {
  [0]=>
  int(7)
  [1]=>
  int(4)
}

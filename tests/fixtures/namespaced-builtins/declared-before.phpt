--TEST--
A function of the namespace with a built-in's name, declared before the call: the call means it.
--FILE--
<?php
// A function of the namespace with a built-in's name, declared before the
// call: the call means it.
namespace App;
function is_null($value) { return "App\\is_null(" . var_export($value, true) . ")"; }
function strlen($value) { return -1; }
function check($value) { return [is_null($value), strlen('abc')]; }
for ($round = 0; $round < 2; $round++) {
    var_dump(check(null));
    var_dump(\is_null(null), \strlen('abc'));
}
--EXPECT--
array(2) {
  [0]=>
  string(17) "App\is_null(NULL)"
  [1]=>
  int(-1)
}
bool(true)
int(3)
array(2) {
  [0]=>
  string(17) "App\is_null(NULL)"
  [1]=>
  int(-1)
}
bool(true)
int(3)

--TEST--
Declared after a call site has run: that site keeps the global built-in, as PHP keeps the first target a call site found; a site that has not run yet finds the namespace's function.
--FILE--
<?php
// Declared after a call site has run: that site keeps the global built-in,
// as PHP keeps the first target a call site found; a site that has not run
// yet finds the namespace's function.
namespace App;
function check($value) { return is_null($value); }
function length($value) { return strlen($value); }
var_dump(check(null), length('abc'));
if (true) {
    function is_null($value) { return 'App\is_null'; }
    function strlen($value) { return 'App\strlen'; }
}
var_dump(check(null), length('abc'));
function fresh($value) { return [is_null($value), strlen($value)]; }
var_dump(fresh('x'));
var_dump(check(null));
--EXPECT--
bool(true)
int(3)
bool(true)
int(3)
array(2) {
  [0]=>
  string(11) "App\is_null"
  [1]=>
  string(10) "App\strlen"
}
bool(true)

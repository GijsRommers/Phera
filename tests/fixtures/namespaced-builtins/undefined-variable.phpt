--TEST--
An undefined variable as the argument: one warning, on the call's line, as a value and as a condition.
--FILE--
<?php
// An undefined variable as the argument: one warning, on the call's line,
// as a value and as a condition.
namespace App;
function check() {
    for ($round = 0; $round < 2; $round++) {
        var_dump(is_null($missing));
        if (is_null($missing)) { echo "null\n"; }
        if (!is_array($missing)) { echo "not an array\n"; }
        var_dump(strlen($missing ?? ''));
    }
}
check();
--EXPECTF--

Warning: Undefined variable $missing in %s on line 7
bool(true)

Warning: Undefined variable $missing in %s on line 8
null

Warning: Undefined variable $missing in %s on line 9
not an array
int(0)

Warning: Undefined variable $missing in %s on line 7
bool(true)

Warning: Undefined variable $missing in %s on line 8
null

Warning: Undefined variable $missing in %s on line 9
not an array
int(0)

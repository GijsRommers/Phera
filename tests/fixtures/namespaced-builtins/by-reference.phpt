--TEST--
A namespace's function with a by-reference parameter receives the variable, not a copy.
--FILE--
<?php
// A namespace's function with a by-reference parameter receives the
// variable, not a copy.
namespace App;
function is_array(&$value) { $value = 'changed'; return true; }
$subject = [1];
for ($round = 0; $round < 2; $round++) {
    var_dump(is_array($subject), $subject);
    $subject = [1];
}
--EXPECT--
bool(true)
string(7) "changed"
bool(true)
string(7) "changed"

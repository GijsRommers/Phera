--TEST--
A property coalescing assignment converts the key during each PHP access
--FILE--
<?php
function default_value() { echo "fallback\n"; return 'default'; }
class Box { public array $data = []; }
$b = new Box;
var_dump($b->data[1.5] ??= default_value());
var_dump($b->data[1.5] ??= default_value());
--EXPECTF--
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 5
fallback

Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 5
string(7) "default"

Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 6
string(7) "default"

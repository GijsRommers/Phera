--TEST--
Deprecated expression interpolation reads a variable by computed name
--FILE--
<?php
echo "starting\n";
$name = 'value';
$key = 'name';
function show($key) {
    $name = 'local';
    echo "${$key} ${($key)}\n";
}
show($key);
show($key);
--EXPECTF--
Deprecated: Using ${expr} (variable variables) in strings is deprecated, use {${expr}} instead in /private%s on line 7

Deprecated: Using ${expr} (variable variables) in strings is deprecated, use {${expr}} instead in /private%s on line 7
starting
local local
local local

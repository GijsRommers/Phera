--TEST--
Special array_key_exists reads compiled locals after the other expression runs
--FILE--
<?php
$key = 'before';
$array = function () use (&$key) { $key = 'after'; return ['after' => null]; };
var_dump(array_key_exists($key, $array()));
var_dump(array_key_exists('1', [1 => null]));
set_error_handler(function ($level, $message) { echo "$message\n"; return true; });
var_dump(array_key_exists(null, ['' => true]));
--EXPECT--
bool(true)
bool(true)
Using null as the key parameter for array_key_exists() is deprecated, use an empty string instead
bool(true)

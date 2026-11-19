--TEST--
A direct callback resolves first and warns about reference arguments at the send
--FILE--
<?php
set_error_handler(function ($level, $message) { echo "$message\n"; return true; });
function referenceCallback(&$value, $second): int { $value++; return $value + $second; }
function secondCallbackArgument(): int { echo "evaluated\n"; return 7; }
try {
    call_user_func('undefined_native_callback', secondCallbackArgument());
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
$value = 1;
$name = 'referenceCallback';
var_dump(call_user_func($name, $value, secondCallbackArgument()), $value);
var_dump(call_user_func_array($name, ['value' => &$value, 'second' => 8]), $value);
try {
    call_user_func_array($name, false);
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
call_user_func(): Argument #1 ($callback) must be a valid callback, function "undefined_native_callback" not found or invalid function name
referenceCallback(): Argument #1 ($value) must be passed by reference, value given
evaluated
int(9)
int(1)
int(10)
int(2)
call_user_func_array(): Argument #2 ($args) must be of type array, false given

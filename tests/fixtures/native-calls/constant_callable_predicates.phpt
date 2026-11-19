--TEST--
Callable predicates retain runtime lookup for names declared later
--FILE--
<?php
var_dump(function_exists('ABS'), is_callable('strlen'));
var_dump(function_exists('native_calls_later'), is_callable('native_calls_later'));
if (true) { function native_calls_later() {} }
var_dump(function_exists('native_calls_later'), is_callable('native_calls_later'));
--EXPECT--
bool(true)
bool(true)
bool(false)
bool(false)
bool(true)
bool(true)

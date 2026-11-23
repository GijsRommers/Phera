--TEST--
Function exists spellings
--FILE--
<?php
function myFunc(): void {}
$name = 'strlen';
var_dump(function_exists($name));
var_dump(function_exists('STRLEN'));
var_dump(function_exists('\\strlen'));
var_dump(function_exists('myFunc'));
var_dump(function_exists('MYFUNC'));
var_dump(function_exists('nope'));
var_dump(function_exists(str_repeat('n', 70) . 'ope'));
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(false)
bool(false)

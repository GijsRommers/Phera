--TEST--
Arrays and variadic names are tried in input order
--FILE--
<?php
var_dump(setlocale(LC_ALL, ['cannot exist !', 'C'], 'does not exist !'));
var_dump(setlocale(LC_CTYPE, [], 'C'));
--EXPECT--
string(1) "C"
string(1) "C"

--TEST--
An overlong locale warns and later candidates still apply
--FILE--
<?php
var_dump(setlocale(LC_ALL, str_repeat('x', 255), 'C'));
--EXPECTF--
Warning: setlocale(): Specified locale name is too long in %s/invalid-name.php on line 2
string(1) "C"

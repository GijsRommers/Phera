--TEST--
An empty or null name selects the environment locale
--FILE--
<?php
putenv('LC_ALL=C');
var_dump(setlocale(LC_NUMERIC, ''));
var_dump(setlocale(LC_NUMERIC, null));
--EXPECT--
string(1) "C"
string(1) "C"

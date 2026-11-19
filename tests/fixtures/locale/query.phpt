--TEST--
An integer or string zero queries a locale category
--FILE--
<?php
var_dump(setlocale(LC_ALL, 'C'));
var_dump(setlocale(LC_NUMERIC, 0), setlocale(LC_TIME, '0'));
--EXPECT--
string(1) "C"
string(1) "C"
string(1) "C"

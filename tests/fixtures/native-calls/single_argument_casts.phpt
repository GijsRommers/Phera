--TEST--
Single-argument casts preserve ordinary calls for named and spread arguments
--FILE--
<?php
var_dump(intval('1e3'), intval('ff', 16), floatval('2.5'), doubleval('3.5'));
var_dump(boolval('0'), strval(100), strlen(string: 'abc'), count(...[[1, 2]]));
--EXPECT--
int(1000)
int(255)
float(2.5)
float(3.5)
bool(false)
string(3) "100"
int(3)
int(2)

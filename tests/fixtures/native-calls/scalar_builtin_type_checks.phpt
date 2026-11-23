--TEST--
Direct type checks cover aliases, references, and closed resources
--FILE--
<?php
$value = 42;
$alias =& $value;
var_dump(is_int($alias), is_integer($alias), is_long($alias), is_null($alias));
$value = 1.5;
var_dump(is_int($alias), is_double($alias), is_float($alias), is_scalar($alias));
var_dump(is_bool(false), is_null(null), is_string('42'), is_array([]), is_object(new stdClass), gettype($alias));
$stream = fopen('php://memory', 'w+');
var_dump(is_resource($stream));
fclose($stream);
var_dump(is_resource($stream), gettype($stream));
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(false)
bool(false)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
string(6) "double"
bool(true)
bool(false)
string(17) "resource (closed)"

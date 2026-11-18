--TEST--
The obsolete get_defined_functions parameter still emits a PHP notice
--FILE--
<?php
var_dump(in_array('strlen', get_defined_functions(false)['internal'], true));
--EXPECTF--
Deprecated: get_defined_functions(): The $exclude_disabled parameter has no effect since PHP 8.0 in %s on line 2
bool(true)

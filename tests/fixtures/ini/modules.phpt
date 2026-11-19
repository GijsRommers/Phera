--TEST--
Module versions, case-insensitive lookup, and functions
--FILE--
<?php
var_dump(phpversion() === phpversion('CoRE'), extension_loaded('DaTe'));
var_dump(phpversion('missing'), get_extension_funcs('missing'), get_extension_funcs('Reflection'));
var_dump(in_array('date_default_timezone_get', get_extension_funcs('DaTe'), true));
var_dump(in_array('date', get_loaded_extensions(), true));
--EXPECT--
bool(true)
bool(true)
bool(false)
bool(false)
bool(false)
bool(true)
bool(true)

--TEST--
Omitted functions and constants are absent from introspection
--FILE--
<?php
// This fixture asserts behavior specific to Phera’s deprecated-item policy.
var_dump(function_exists('LCG_VALUE'), defined('MT_RAND_PHP'));
var_dump(method_exists('Redis', 'sortAsc'), is_callable('lcg_value'));
var_dump(defined('ZipArchive::FL_RECOMPRESS'));
var_dump(in_array('lcg_value', get_defined_functions()['internal'], true));
var_dump(array_key_exists('MT_RAND_PHP', get_defined_constants()));
var_dump(array_key_exists('MT_RAND_PHP', get_defined_constants(true)['random']));
--EXPECT--
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)

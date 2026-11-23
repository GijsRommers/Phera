--TEST--
Memory functions report positive byte counts and reset the observed peak
--FILE--
<?php
var_dump(memory_get_usage() > 0, memory_get_usage(true) > 0);
var_dump(memory_get_peak_usage() > 0, memory_get_peak_usage(true) > 0);
var_dump(memory_reset_peak_usage(), memory_get_peak_usage() > 0);
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
NULL
bool(true)

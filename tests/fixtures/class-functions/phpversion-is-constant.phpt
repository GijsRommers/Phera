--TEST--
Phpversion is constant
--SKIPIF--
<?php if (PHP_VERSION_ID < 80513) die('skip PHP behaviour since 8.5.13'); ?>
--FILE--
<?php
var_dump(phpversion(),phpversion()===PHP_VERSION);
--EXPECT--
string(6) "8.5.13"
bool(true)

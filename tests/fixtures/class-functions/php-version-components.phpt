--TEST--
Php version components
--SKIPIF--
<?php if (PHP_VERSION_ID < 80513) die('skip PHP behaviour since 8.5.13'); ?>
--FILE--
<?php
var_dump(PHP_MAJOR_VERSION,PHP_MINOR_VERSION,PHP_RELEASE_VERSION,PHP_VERSION_ID,PHP_EXTRA_VERSION);
--EXPECT--
int(8)
int(5)
int(13)
int(80513)
string(0) ""

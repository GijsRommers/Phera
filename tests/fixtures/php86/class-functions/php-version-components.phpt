--TEST--
Php version components
--FILE--
<?php
var_dump(PHP_MAJOR_VERSION,PHP_MINOR_VERSION,PHP_RELEASE_VERSION,PHP_VERSION_ID,PHP_EXTRA_VERSION);
--EXPECT--
int(8)
int(6)
int(0)
int(80600)
string(3) "RC7"

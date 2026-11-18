--TEST--
Php version components
--FILE--
<?php
var_dump(PHP_MAJOR_VERSION,PHP_MINOR_VERSION,PHP_RELEASE_VERSION,PHP_VERSION_ID,PHP_EXTRA_VERSION);
--EXPECT--
int(8)
int(5)
int(12)
int(80512)
string(0) ""

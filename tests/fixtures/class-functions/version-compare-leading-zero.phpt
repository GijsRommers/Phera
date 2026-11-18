--TEST--
Version compare leading zero
--FILE--
<?php
var_dump(version_compare("8.05","8.5","eq"));
--EXPECT--
bool(true)

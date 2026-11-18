--TEST--
Alias missing warning
--FILE--
<?php
var_dump(class_alias("Missing", "Another"));
--EXPECTF--

Warning: Class "Missing" not found in %s/alias-missing-warning.php on line 2
bool(false)

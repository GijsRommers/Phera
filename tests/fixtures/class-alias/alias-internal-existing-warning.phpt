--TEST--
Alias internal existing warning
--FILE--
<?php
var_dump(class_alias("stdClass", "Exception"));
--EXPECTF--

Warning: Cannot redeclare class Exception in %s/alias-internal-existing-warning.php on line 2
bool(false)

--TEST--
Class parents missing no autoload
--FILE--
<?php
var_dump(class_parents("Missing", false));
--EXPECTF--

Warning: class_parents(): Class Missing does not exist in %s/class-parents-missing-no-autoload.php on line 2
bool(false)

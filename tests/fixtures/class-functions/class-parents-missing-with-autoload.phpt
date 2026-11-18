--TEST--
Class parents missing with autoload
--FILE--
<?php
var_dump(class_parents("Missing"));
--EXPECTF--

Warning: class_parents(): Class Missing does not exist and could not be loaded in %s/class-parents-missing-with-autoload.php on line 2
bool(false)

--TEST--
Alias no load switch
--FILE--
<?php
spl_autoload_register(fn($name) => print($name)); var_dump(class_alias("Missing", "Another", false));
--EXPECTF--

Warning: Class "Missing" not found in %s/alias-no-load-switch.php on line 2
bool(false)

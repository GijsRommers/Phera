--TEST--
Callback return does not declare
--FILE--
<?php
spl_autoload_register(fn($name) => true); var_dump(class_exists("Missing"));
--EXPECT--
bool(false)

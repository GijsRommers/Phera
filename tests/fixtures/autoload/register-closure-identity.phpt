--TEST--
Register closure identity
--FILE--
<?php
$a = function ($name) {}; $b = function ($name) {}; spl_autoload_register($a); spl_autoload_register($a); spl_autoload_register($b); var_dump(count(spl_autoload_functions()));
--EXPECT--
int(2)

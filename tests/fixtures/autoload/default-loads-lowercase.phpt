--TEST--
Default loads lowercase
--FILE--
<?php
set_include_path(__DIR__); spl_autoload_register(); var_dump(class_exists("SPLFileProbe"));
--EXPECT--
bool(true)

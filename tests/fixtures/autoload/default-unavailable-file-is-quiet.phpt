--TEST--
Default unavailable file is quiet
--FILE--
<?php
set_include_path(__DIR__); spl_autoload_register(); var_dump(class_exists("SPLUnreachableProbe"));
--EXPECT--
bool(false)

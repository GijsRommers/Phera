--TEST--
Default explicit extension
--FILE--
<?php
set_include_path(__DIR__); var_dump(spl_autoload("SPLAlternateProbe", ".inc"), class_exists("SPLAlternateProbe", false));
--EXPECT--
NULL
bool(true)

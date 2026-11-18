--TEST--
Alias internal interface
--FILE--
<?php
var_dump(class_alias("Throwable", "Another"), interface_exists("Another", false));
--EXPECT--
bool(true)
bool(true)

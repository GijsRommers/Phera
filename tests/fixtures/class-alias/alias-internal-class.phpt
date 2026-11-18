--TEST--
Alias internal class
--FILE--
<?php
var_dump(class_alias("stdClass", "Another"), get_class(new Another));
--EXPECT--
bool(true)
string(8) "stdClass"

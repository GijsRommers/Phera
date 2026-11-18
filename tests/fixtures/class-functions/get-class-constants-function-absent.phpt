--TEST--
Get class constants function absent
--FILE--
<?php
var_dump(function_exists("get_class_constants"));
--EXPECT--
bool(false)

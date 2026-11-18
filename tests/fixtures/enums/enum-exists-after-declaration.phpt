--TEST--
enums: enum exists after declaration
--FILE--
<?php
var_dump(enum_exists("E")); enum E{} var_dump(enum_exists("E"));
--EXPECT--
bool(false)
bool(true)

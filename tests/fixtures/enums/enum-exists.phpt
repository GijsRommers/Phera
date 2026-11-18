--TEST--
enums: enum exists
--FILE--
<?php
enum E{} var_dump(enum_exists("e"),class_exists("E"));
--EXPECT--
bool(true)
bool(true)

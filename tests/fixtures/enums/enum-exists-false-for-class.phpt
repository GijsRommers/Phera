--TEST--
enums: enum exists false for class
--FILE--
<?php
class C{} var_dump(enum_exists("C"));
--EXPECT--
bool(false)

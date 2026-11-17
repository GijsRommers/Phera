--TEST--
Native classes: constant-defined-inherited
--FILE--
<?php
class Child extends PDO {
}
 var_dump(defined("Child::FETCH_ASSOC"),constant("Child::FETCH_ASSOC"));

--EXPECT--
bool(true)
int(2)

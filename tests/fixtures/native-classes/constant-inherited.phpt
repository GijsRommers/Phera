--TEST--
Native classes: constant-inherited
--FILE--
<?php
class Child extends PDO {
}
 class Grandchild extends Child {
}
 var_dump(Child::FETCH_ASSOC, Grandchild::FETCH_NUM);

--EXPECT--
int(2)
int(3)

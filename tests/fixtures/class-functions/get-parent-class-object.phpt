--TEST--
Get parent class object
--FILE--
<?php
class A {} class B extends A {} var_dump(get_parent_class(new B));
--EXPECT--
string(1) "A"

--TEST--
Get parent class root
--FILE--
<?php
class A {} var_dump(get_parent_class(A::class));
--EXPECT--
bool(false)

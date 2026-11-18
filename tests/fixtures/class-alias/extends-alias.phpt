--TEST--
Extends alias
--FILE--
<?php
class Original {} class_alias("Original", "Another"); class Child extends Another {} var_dump(get_parent_class("Child"));
--EXPECT--
string(8) "Original"

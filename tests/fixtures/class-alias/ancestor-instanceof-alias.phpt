--TEST--
Ancestor instanceof alias
--FILE--
<?php
class Original {} class_alias("Original", "Another"); class Child extends Original {} var_dump(new Child instanceof Another);
--EXPECT--
bool(true)

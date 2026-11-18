--TEST--
Dynamic instanceof alias
--FILE--
<?php
class Original {} class_alias("Original", "Another"); $alias = "Another"; var_dump(new Original instanceof $alias);
--EXPECT--
bool(true)

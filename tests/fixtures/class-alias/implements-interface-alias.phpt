--TEST--
Implements interface alias
--FILE--
<?php
interface Original {} class_alias("Original", "Another"); class Child implements Another {} var_dump(new Child instanceof Original);
--EXPECT--
bool(true)

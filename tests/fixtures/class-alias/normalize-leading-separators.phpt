--TEST--
Normalize leading separators
--FILE--
<?php
class Original {} var_dump(class_alias("\\Original", "\\Another"), class_exists("Another", false));
--EXPECT--
bool(true)
bool(true)

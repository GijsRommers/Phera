--TEST--
Alias used as is a target
--FILE--
<?php
class Original {} class_alias("Original", "Another"); var_dump(is_a(new Original, "Another"));
--EXPECT--
bool(true)

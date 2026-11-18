--TEST--
Alias is case insensitive
--FILE--
<?php
class Original {} class_alias("Original", "Another"); var_dump(class_exists("ANOTHER", false));
--EXPECT--
bool(true)

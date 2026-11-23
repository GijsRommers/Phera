--TEST--
Chained alias
--FILE--
<?php
class Original {} class_alias("Original", "Another"); class_alias("Another", "Third"); var_dump(get_class(new Third));
--EXPECT--
string(8) "Original"

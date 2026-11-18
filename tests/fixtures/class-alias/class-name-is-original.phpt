--TEST--
Class name is original
--FILE--
<?php
class Original {} class_alias("Original", "Another"); var_dump(get_class(new Another));
--EXPECT--
string(8) "Original"

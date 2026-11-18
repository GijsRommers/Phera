--TEST--
Return type alias
--FILE--
<?php
class Original {} class_alias("Original", "Another"); function make(): Another { return new Original; } var_dump(get_class(make()));
--EXPECT--
string(8) "Original"

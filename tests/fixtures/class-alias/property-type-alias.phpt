--TEST--
Property type alias
--FILE--
<?php
class Original {} class_alias("Original", "Another"); class Holder { public Another $value; } $holder = new Holder; $holder->value = new Original; var_dump(get_class($holder->value));
--EXPECT--
string(8) "Original"

--TEST--
Return value
--FILE--
<?php
class Original {} var_dump(class_alias("Original", "Another"));
--EXPECT--
bool(true)

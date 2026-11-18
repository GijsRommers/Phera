--TEST--
Instanceof alias
--FILE--
<?php
class Original {} class_alias("Original", "Another"); var_dump(new Original instanceof Another);
--EXPECT--
bool(true)

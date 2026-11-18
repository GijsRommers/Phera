--TEST--
Alias is not own subclass
--FILE--
<?php
class Original {} class_alias("Original", "Another"); var_dump(is_subclass_of("Original", "Another"));
--EXPECT--
bool(false)

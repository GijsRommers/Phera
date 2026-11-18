--TEST--
Original name is case insensitive
--FILE--
<?php
class Original {} var_dump(class_alias("ORIGINAL", "Another"));
--EXPECT--
bool(true)

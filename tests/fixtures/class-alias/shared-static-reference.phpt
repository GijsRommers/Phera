--TEST--
Shared static reference
--FILE--
<?php
class Original { public static $value = 1; } class_alias("Original", "Another"); $x =& Another::$value; $x = 9; var_dump(Original::$value);
--EXPECT--
int(9)

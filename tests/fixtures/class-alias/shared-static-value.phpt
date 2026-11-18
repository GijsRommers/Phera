--TEST--
Shared static value
--FILE--
<?php
class Original { public static int $value = 1; } class_alias("Original", "Another"); Another::$value++; var_dump(Original::$value);
--EXPECT--
int(2)

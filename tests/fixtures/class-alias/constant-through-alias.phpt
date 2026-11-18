--TEST--
Constant through alias
--FILE--
<?php
class Original { const VALUE = 6; } class_alias("Original", "Another"); var_dump(Another::VALUE);
--EXPECT--
int(6)

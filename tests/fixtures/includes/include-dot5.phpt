--TEST--
include-dot5
--FILE--
<?php var_dump(@include './return5.inc'); var_dump(include 'return5.inc');
--EXPECT--
int(5)
int(5)

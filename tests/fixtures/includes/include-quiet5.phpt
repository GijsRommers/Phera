--TEST--
include-quiet5
--FILE--
<?php var_dump(@include 'missing5.php'); var_dump(@include_once 'missing5.php');
--EXPECT--
bool(false)
bool(false)

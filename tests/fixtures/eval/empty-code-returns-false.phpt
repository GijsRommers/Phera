--TEST--
Empty code returns false
--FILE--
<?php
var_dump(eval(''));
--EXPECT--
bool(false)

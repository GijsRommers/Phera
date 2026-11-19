--TEST--
A masked diagnostic is still recorded
--FILE--
<?php
error_reporting(0);
echo $missing;
echo error_get_last()['message'], "\n";
--EXPECT--
Undefined variable $missing

--TEST--
Exceptions: finally_nested_return
--FILE--
<?php
function run(){try {try {return 3;} finally {echo "inner";}} finally {echo "outer";}} echo run();
--EXPECTF--
innerouter3
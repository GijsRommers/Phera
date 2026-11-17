--TEST--
Exceptions: finally_return
--FILE--
<?php
function run() {try {return 7;} finally {echo "cleanup\n";}} echo run();
--EXPECTF--
cleanup
7
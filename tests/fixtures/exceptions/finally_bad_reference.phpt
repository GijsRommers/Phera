--TEST--
Exceptions: finally_bad_reference
--FILE--
<?php
function &run(){try{return 2;}finally{echo "cleanup";}}$value=&run();echo $value;
--EXPECTF--
cleanup
Notice: Only variable references should be returned by reference in %s on line 2
2
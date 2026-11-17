--TEST--
Exceptions: native_tostring
--FILE--
<?php
$error=new Exception("x");echo $error,"\n",(string)$error;
--EXPECTF--
Exception: x in %s:2
Stack trace:
#0 {main}
Exception: x in %s:2
Stack trace:
#0 {main}
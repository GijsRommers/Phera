--TEST--
Exceptions: finally_reference
--FILE--
<?php
function &run(&$value){try{return $value;}finally{$value=2;}} $value=1;$result=&run($value);$result=3;echo $value;
--EXPECTF--
3
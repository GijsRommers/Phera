--TEST--
Exceptions: finally_invalid_return_previous
--FILE--
<?php
function run():int{try{throw new Exception("original");}finally{return [];}}try{run();}catch(Throwable $error){echo get_class($error),"|",get_class($error->getPrevious()),"|",$error->getPrevious()->getMessage();}
--EXPECTF--
TypeError|Exception|original
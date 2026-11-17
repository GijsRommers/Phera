--TEST--
Exceptions: finally_shared_previous
--FILE--
<?php
try{$first=new Exception("first");try{throw new Exception("second",0,$first);}finally{throw new Exception("third",0,$first);}}catch(Throwable $error){while($error){echo $error->getMessage(),"|";$error=$error->getPrevious();}}
--EXPECTF--
third|first|
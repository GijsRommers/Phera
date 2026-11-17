--TEST--
Exceptions: finally_throw_previous
--FILE--
<?php
try{try{throw new Exception("first");}finally{throw new Exception("third",0,new Exception("second"));}}catch(Throwable $error){while($error){echo $error->getMessage(),"|";$error=$error->getPrevious();}}
--EXPECTF--
third|second|first|
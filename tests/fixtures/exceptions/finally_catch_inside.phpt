--TEST--
Exceptions: finally_catch_inside
--FILE--
<?php
try{try{throw new Exception("original");}finally{try{throw new Exception("local");}catch(Throwable $error){echo $error->getMessage();}}}catch(Throwable $error){echo $error->getMessage();}
--EXPECTF--
localoriginal
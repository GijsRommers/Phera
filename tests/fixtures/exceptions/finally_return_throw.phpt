--TEST--
Exceptions: finally_return_throw
--FILE--
<?php
function run(){try{return 3;}finally{throw new Exception("cleanup");}}try{run();}catch(Throwable $error){echo $error->getMessage();var_dump($error->getPrevious());}
--EXPECTF--
cleanupNULL

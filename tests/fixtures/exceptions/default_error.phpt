--TEST--
Exceptions: default_error
--FILE--
<?php
function run($object=new MissingClass){}try{run();}catch(Throwable $error){echo get_class($error),"|",$error->getMessage();}
--EXPECTF--
Error|Class "MissingClass" not found
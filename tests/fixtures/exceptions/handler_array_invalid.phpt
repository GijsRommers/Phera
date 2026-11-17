--TEST--
Exceptions: handler_array_invalid
--FILE--
<?php
try{set_exception_handler(["",""]);}catch(Throwable $error){echo $error->getMessage();}
--EXPECTF--
set_exception_handler(): Argument #1 ($callback) must be a valid callback or null, class "" not found
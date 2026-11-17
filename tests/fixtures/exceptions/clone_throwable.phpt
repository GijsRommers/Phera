--TEST--
Exceptions: clone_throwable
--FILE--
<?php
try{$value=clone new Exception("x");}catch(Error $error){echo $error->getMessage();}
--EXPECTF--
Trying to clone an uncloneable object of class Exception
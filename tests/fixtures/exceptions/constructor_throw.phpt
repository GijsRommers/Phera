--TEST--
Exceptions: constructor_throw
--FILE--
<?php
class Example {function __construct(){throw new Exception("constructor");}} try {$object=new Example;} catch(Throwable $error){echo $error->getMessage();} echo "ok";
--EXPECTF--
constructorok
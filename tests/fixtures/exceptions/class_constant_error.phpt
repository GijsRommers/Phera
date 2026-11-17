--TEST--
Exceptions: class_constant_error
--FILE--
<?php
class Example{const NUMBER=1/0;}try{echo Example::NUMBER;}catch(Throwable $error){echo get_class($error),"|",$error->getMessage();}
--EXPECTF--
DivisionByZeroError|Division by zero
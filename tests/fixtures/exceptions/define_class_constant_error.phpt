--TEST--
Exceptions: define_class_constant_error
--FILE--
<?php
try { define("Thing::VALUE", 1); } catch (Throwable $error) { echo get_class($error), "|", $error->getMessage(), "|", $error->getCode(), "\n", $error->getTraceAsString(), "\n"; }
--EXPECTF--
ValueError|define(): Argument #1 ($constant_name) cannot be a class constant|0
#0 %s(2): define('Thing::VALUE', 1)
#1 {main}

--TEST--
Classes: native-exception
--FILE--
<?php $error=new Exception("hello", 7); echo $error->getMessage(), ":", $error->getCode();
--EXPECT--
hello:7
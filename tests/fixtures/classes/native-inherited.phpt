--TEST--
Classes: native-inherited
--FILE--
<?php class CustomError extends Exception {} $error=new CustomError("hello"); echo $error->getMessage();
--EXPECT--
hello
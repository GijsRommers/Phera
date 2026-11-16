--TEST--
Classes: native-instanceof
--FILE--
<?php $error=new TypeError("hello"); var_dump($error instanceof Error, $error instanceof Throwable);
--EXPECT--
bool(true)
bool(true)

--TEST--
callable syntax only object
--FILE--
<?php
var_dump(is_callable(new stdClass,true));class A{function __invoke(){}}var_dump(is_callable(new A,true));
--EXPECT--
bool(false)
bool(true)

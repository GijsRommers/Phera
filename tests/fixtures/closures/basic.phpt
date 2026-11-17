--TEST--
basic
--FILE--
<?php
$f=function(){return 42;}; var_dump($f());
--EXPECT--
int(42)

--TEST--
empty
--FILE--
<?php
$f=function(){};var_dump($f());
--EXPECT--
NULL

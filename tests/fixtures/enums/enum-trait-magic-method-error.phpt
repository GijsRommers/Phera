--TEST--
enums: enum trait magic method error
--FILE--
<?php
trait T{function __get($name){}} enum E{use T;}
--EXPECTF--
Fatal error: Enum E cannot include magic method __get in %s on line 2

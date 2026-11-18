--TEST--
enums: enum get method forbidden
--FILE--
<?php
enum E{function __get($p){}}
--EXPECTF--
Fatal error: Enum E cannot include magic method __get in %s on line 2

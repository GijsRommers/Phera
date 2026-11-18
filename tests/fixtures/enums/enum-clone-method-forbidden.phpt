--TEST--
enums: enum clone method forbidden
--FILE--
<?php
enum E{function __clone(){}}
--EXPECTF--
Fatal error: Enum E cannot include magic method __clone in %s on line 2

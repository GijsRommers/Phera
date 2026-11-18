--TEST--
enums: enum constructor forbidden
--FILE--
<?php
enum E{function __construct(){}}
--EXPECTF--
Fatal error: Enum E cannot include magic method __construct in %s on line 2

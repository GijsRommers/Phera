--TEST--
enums: enum promoted property forbidden
--FILE--
<?php
enum E {function __construct(public int $a) {}}
--EXPECTF--
Fatal error: Enum E cannot include properties in %s on line 2

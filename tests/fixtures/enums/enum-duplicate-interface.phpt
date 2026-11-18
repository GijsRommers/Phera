--TEST--
enums: enum duplicate interface
--FILE--
<?php
interface I{} enum E implements I,I {}
--EXPECTF--
Fatal error: Enum E cannot implement previously implemented interface I in /private%s on line 2

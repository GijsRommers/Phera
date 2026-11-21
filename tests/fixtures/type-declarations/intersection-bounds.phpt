--TEST--
Intersection bounds must be classes
--FILE--
<?php
function invalid(int&Stringable $value) {}
--EXPECTF--
Fatal error: Type int cannot be part of an intersection type in %s on line 2

--TEST--
enums: enum trait constant
--FILE--
<?php
trait T{const N=3;} enum E{use T;case A;} echo E::N;
--EXPECT--
3

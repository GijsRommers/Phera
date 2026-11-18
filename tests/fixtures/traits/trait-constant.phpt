--TEST--
traits: trait constant
--FILE--
<?php
trait T{const VALUE=8;} class C{use T;} echo C::VALUE;
--EXPECT--
8

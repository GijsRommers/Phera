--TEST--
traits: matching trait constant
--FILE--
<?php
trait T{const X=3;} class C{use T; const X=3;} echo C::X;
--EXPECT--
3

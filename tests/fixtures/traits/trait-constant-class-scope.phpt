--TEST--
traits: trait constant class scope
--FILE--
<?php
trait T{const CLASS_NAME=__CLASS__;} class C{use T;} echo C::CLASS_NAME;
--EXPECT--
C

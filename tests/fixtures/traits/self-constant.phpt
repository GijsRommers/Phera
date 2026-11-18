--TEST--
traits: self constant
--FILE--
<?php
trait T{function m(){return self::VALUE;}} class C{use T; const VALUE=4;} echo (new C)->m();
--EXPECT--
4

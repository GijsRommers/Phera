--TEST--
traits: property default
--FILE--
<?php
trait T {public $m = 3;} class C {use T;} echo (new C)->m;
--EXPECT--
3

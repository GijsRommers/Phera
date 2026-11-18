--TEST--
traits: compatible trait properties
--FILE--
<?php
trait T {public $p=1;} trait U{public $p=1;} class C{use T,U;} echo (new C)->p;
--EXPECT--
1

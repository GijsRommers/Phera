--TEST--
traits: protected property scope
--FILE--
<?php
trait T { protected $m=3; } class C{use T;} class D extends C { function m(){return $this->m;} } echo (new D)->m();
--EXPECT--
3

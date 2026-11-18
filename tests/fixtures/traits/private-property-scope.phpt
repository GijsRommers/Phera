--TEST--
traits: private property scope
--FILE--
<?php
trait T { private $m=3; function m(){return $this->m;} } class C{use T;} echo (new C)->m();
--EXPECT--
3

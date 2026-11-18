--TEST--
traits: private inherited property
--FILE--
<?php
trait T{private $x=1; function viaT(){return $this->x;}} class P{private $x=2;function viaP(){return $this->x;}} class C extends P{use T;} $c=new C; echo $c->viaT(),$c->viaP();
--EXPECT--
12

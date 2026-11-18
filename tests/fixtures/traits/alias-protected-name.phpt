--TEST--
traits: alias protected name
--FILE--
<?php
trait T{function m(){return 3;}} class C{use T {m as protected n;}} class D extends C{function check(){return $this->n();}} echo (new D)->check();
--EXPECT--
3

--TEST--
traits: abstract method relative signature
--FILE--
<?php
trait T{abstract private function m(self $x): self;} class C {use T;private function m(self $x):self{return $x;}} echo "ok";
--EXPECT--
ok

--TEST--
traits: method error consumer name
--FILE--
<?php
trait T { function m():int{return "text";} } class C {use T;} try{(new C)->m();}catch(TypeError $e){echo $e->getMessage();}
--EXPECT--
C::m(): Return value must be of type int, string returned

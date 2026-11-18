--TEST--
traits: typed property type error for lazy default
--FILE--
<?php
trait T{public int $x=S;} const S="bad"; class C{use T;} try{var_dump((new C)->x);}catch(TypeError $e){echo $e->getMessage();}
--EXPECT--
Cannot assign string to property C::$x of type int

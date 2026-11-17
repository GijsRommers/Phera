--TEST--
Typed reference bookkeeping: inherited-unset
--FILE--
<?php
class P { public int $n = 1;
} class C extends P {}
$c = new C;
$r =& $c->n;
try { $r = [];
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
unset($c->n);
$r = "free";
var_dump($r);
--EXPECT--
Cannot assign array to reference held by property P::$n of type int
string(4) "free"

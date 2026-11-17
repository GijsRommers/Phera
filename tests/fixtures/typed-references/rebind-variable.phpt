--TEST--
Typed reference bookkeeping: rebind-variable
--FILE--
<?php
class C { public int $n = 0;
}
$c = new C;
$r =& $c->n;
$old =& $r;
$other = 5;
$r =& $other;
$r = "free";
try { $old = [];
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
var_dump($r, $c->n);
--EXPECT--
Cannot assign array to reference held by property C::$n of type int
string(4) "free"
int(0)

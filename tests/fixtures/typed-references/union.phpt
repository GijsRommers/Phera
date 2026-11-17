--TEST--
Typed reference bookkeeping: union
--FILE--
<?php
class C { public string|int $n = 0;
}
$c = new C;
$r =& $c->n;
$r = "text";
var_dump($c->n);
$r = 2;
try { $r = [];
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
var_dump($c->n);
--EXPECT--
string(4) "text"
Cannot assign array to reference held by property C::$n of type string|int
int(2)

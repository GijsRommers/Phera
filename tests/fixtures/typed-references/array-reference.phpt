--TEST--
Typed reference bookkeeping: array-reference
--FILE--
<?php
class C { public int $n = 0;
}
$c = new C;
$a = [];
$a[0] =& $c->n;
try { $a[0] = [];
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
unset($c->n);
$a[0] = "free";
var_dump($a[0]);
--EXPECT--
Cannot assign array to reference held by property C::$n of type int
string(4) "free"

--TEST--
Typed reference bookkeeping: array-copy-reference
--FILE--
<?php
class C { public int $n = 0;
}
$c = new C;
$a = [];
$a[0] =& $c->n;
$b = $a;
unset($a);
try { $b[0] = [];
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
unset($c);
$b[0] = "free";
var_dump($b[0]);
--EXPECT--
Cannot assign array to reference held by property C::$n of type int
string(4) "free"

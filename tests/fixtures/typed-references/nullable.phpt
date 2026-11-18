--TEST--
Typed reference bookkeeping: nullable
--FILE--
<?php
class C { public ?int $n = 0;
}
$c = new C;
$r =& $c->n;
$r = null;
var_dump($c->n);
$r = 2;
try { $r = [];
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
var_dump($c->n);
--EXPECT--
NULL
Cannot assign array to reference held by property C::$n of type ?int
int(2)

--TEST--
Typed reference bookkeeping: object-id-reuse
--FILE--
<?php
class C { public int $n = 1;
}
for ($i = 0;
$i < 5;
$i++) { $c = new C;
$r =& $c->n;
unset($c);
$r = "free";
}
$c = new C;
$next =& $c->n;
$r = [];
var_dump($r);
try { $next = [];
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
--EXPECT--
array(0) {
}
Cannot assign array to reference held by property C::$n of type int

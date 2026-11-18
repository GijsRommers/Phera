--TEST--
Typed reference bookkeeping: unset-one-holder
--FILE--
<?php
class C { public int $n = 0;
public ?int $maybe = 0;
}
$c = new C;
$r =& $c->n;
$c->maybe =& $r;
unset($c->n);
$r = null;
try { $r = [];
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
unset($c->maybe);
$r = [];
var_dump($r);
--EXPECT--
Cannot assign array to reference held by property C::$maybe of type ?int
array(0) {
}

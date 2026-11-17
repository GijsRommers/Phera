--TEST--
Typed reference bookkeeping: multiple-declarations
--FILE--
<?php
class C { public int $n = 1;
public ?int $maybe = 2;
}
$c = new C;
$r =& $c->n;
$c->maybe =& $r;
$r = 3;
try { $r = null;
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
var_dump($c->n, $c->maybe);
--EXPECT--
Cannot assign null to reference held by property C::$n of type int
int(3)
int(3)

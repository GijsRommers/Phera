--TEST--
Typed reference bookkeeping: strict-write
--FILE--
<?php
declare(strict_types=1);
class C { public int $n = 0;
}
$c = new C;
$r =& $c->n;
try { $r = "12";
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
var_dump($c->n);
--EXPECT--
Cannot assign string to reference held by property C::$n of type int
int(0)

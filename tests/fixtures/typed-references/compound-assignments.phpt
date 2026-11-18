--TEST--
Typed reference bookkeeping: compound-assignments
--FILE--
<?php
class C { public int $n = 2;
}
$c = new C;
$r =& $c->n;
$r += 3;
$r *= 4;
var_dump($c->n);
try { $r .= "abc";
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
var_dump($c->n);
--EXPECT--
int(20)
Cannot assign string to reference held by property C::$n of type int
int(20)

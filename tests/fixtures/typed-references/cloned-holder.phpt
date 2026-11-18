--TEST--
Typed reference bookkeeping: cloned-holder
--FILE--
<?php
class C { public int $n = 0;
}
$c = new C;
$r =& $c->n;
$copy = clone $c;
unset($c);
try { $r = [];
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
$r = 5;
var_dump($copy->n);
unset($copy);
$r = "free";
var_dump($r);
--EXPECT--
Cannot assign array to reference held by property C::$n of type int
int(5)
string(4) "free"

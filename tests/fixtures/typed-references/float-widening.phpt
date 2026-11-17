--TEST--
Typed reference bookkeeping: float-widening
--FILE--
<?php
class C { public float $n = 0.0;
}
$c = new C;
$r =& $c->n;
$r = 3;
var_dump($r, $c->n);
$r += 2;
var_dump($r);
--EXPECT--
float(3)
float(3)
float(5)

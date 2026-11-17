--TEST--
Typed reference bookkeeping: released-alias-clone
--FILE--
<?php
class C { public int $n = 1;
}
$c = new C;
$r =& $c->n;
unset($r);
$copy = clone $c;
$copy->n = 2;
var_dump($c->n, $copy->n);
--EXPECT--
int(1)
int(2)

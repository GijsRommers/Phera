--TEST--
Typed reference bookkeeping: weak-write
--FILE--
<?php
class C { public int $n = 0;
}
$c = new C;
$r =& $c->n;
$r = "12";
var_dump($r, $c->n);
--EXPECT--
int(12)
int(12)

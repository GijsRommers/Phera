--TEST--
Typed reference bookkeeping: unset-property
--FILE--
<?php
class C { public int $n = 0;
}
$c = new C;
$r =& $c->n;
unset($c->n);
$r = "released";
var_dump($r);
$c->n = 9;
var_dump($c->n);
--EXPECT--
string(8) "released"
int(9)

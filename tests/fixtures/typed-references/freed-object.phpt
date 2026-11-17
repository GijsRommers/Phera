--TEST--
Typed reference bookkeeping: freed-object
--FILE--
<?php
class C { public int $n = 0;
}
$c = new C;
$r =& $c->n;
unset($c);
$r = "released";
var_dump($r);
--EXPECT--
string(8) "released"

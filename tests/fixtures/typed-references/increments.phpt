--TEST--
Typed reference bookkeeping: increments
--FILE--
<?php
class C { public int $n = 0;
}
$c = new C;
$r =& $c->n;
++$r;
$r++;
--$r;
var_dump($r, $c->n);
$r = 40;
var_dump($r++, ++$r, $r--, --$r);
--EXPECT--
int(1)
int(1)
int(40)
int(42)
int(42)
int(40)

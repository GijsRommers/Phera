--TEST--
Method, nullsafe, property and subscript chains of 10,000 steps
--FILE--
<?php
class Counter { public $n = 0; public $self; function step() { $this->n++; return $this; } }
$c = new Counter;
$c->self = $c;
echo eval('return $c' . str_repeat('->step()', 10000) . '->n;'), "\n";
echo eval('return $c' . str_repeat('?->step()', 10000) . '->n;'), "\n";
echo eval('return $c' . str_repeat('->self', 10000) . '->n;'), "\n";
$s = 'x';
echo eval('return $s' . str_repeat('[0]', 10000) . ';'), "\n";
$z = 0;
echo eval('return $z' . str_repeat(' ?: $z', 5000) . ' ?: "last";'), "\n";
--EXPECT--
10000
20000
20000
x
last

--TEST--
operand_order_and_short_circuit
--FILE--
<?php
function f(string $tag, $v) { echo "$tag($v) "; return $v; }
if (f('l', 3) < f('r', 5)) echo "lt\n"; else echo "ge\n";
if (f('l', 7) > f('r', 5)) echo "gt\n"; else echo "le\n";
if (f('l', 7) >= f('r', 5)) echo "ge\n"; else echo "lt2\n";
$zero = 0;
if ($zero && f('never', 1)) echo "and\n"; else echo "short-and\n";
if (f('a', 1) || f('never2', 2)) echo "or\n"; else echo "nor\n";
if (!f('n', 0)) echo "not\n";
$x = f('t', 2) === 2 ? 'yes' : 'no';
echo $x, "\n";
while (f('w', 0) < 2) { echo "round "; break; }
echo "\n";
// operand order inside one comparison with two undefined
function both() { if ($p > $q) return 'gt'; return 'le'; }
echo both(), "\n";
--EXPECTF--
l(3) r(5) lt
l(7) r(5) gt
l(7) r(5) ge
short-and
a(1) or
n(0) not
t(2) yes
w(0) round 

Warning: Undefined variable $q in %s on line 15

Warning: Undefined variable $p in %s on line 15
le

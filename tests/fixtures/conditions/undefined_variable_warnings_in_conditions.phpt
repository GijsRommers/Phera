--TEST--
undefined_variable_warnings_in_conditions
--FILE--
<?php
function w($label): void { echo $label, "\n"; }
if ($u1 === $u2) { w('a'); }
if ($u1 < $u2) { w('b'); }
if ($u1 > $u2) { w('c'); }
if ($u1 >= $u2) { w('d'); }
if ($u1 <= $u2) { w('e'); }
if ($u1 == $u2) { w('f'); }
if (!$u1) { w('g'); }
if ($u1 !== null) { w('h'); }
if ($u1 === null) { w('i'); }
while ($u3 < 2) { w('loop'); $u3 = 5; }
$r = $u4 < 3;
var_dump($r);
$t = $u5 === null ? 'yes' : 'no';
echo $t, "\n";
if ($u6 && $u7) { w('both'); }
if ($u6 || $u7) { w('either'); }
--EXPECTF--

Warning: Undefined variable $u1 in %s on line 3

Warning: Undefined variable $u2 in %s on line 3
a

Warning: Undefined variable $u1 in %s on line 4

Warning: Undefined variable $u2 in %s on line 4

Warning: Undefined variable $u2 in %s on line 5

Warning: Undefined variable $u1 in %s on line 5

Warning: Undefined variable $u2 in %s on line 6

Warning: Undefined variable $u1 in %s on line 6
d

Warning: Undefined variable $u1 in %s on line 7

Warning: Undefined variable $u2 in %s on line 7
e

Warning: Undefined variable $u1 in %s on line 8

Warning: Undefined variable $u2 in %s on line 8
f

Warning: Undefined variable $u1 in %s on line 9
g

Warning: Undefined variable $u1 in %s on line 10

Warning: Undefined variable $u1 in %s on line 11
i

Warning: Undefined variable $u3 in %s on line 12
loop

Warning: Undefined variable $u4 in %s on line 13
bool(true)

Warning: Undefined variable $u5 in %s on line 15
yes

Warning: Undefined variable $u6 in %s on line 17

Warning: Undefined variable $u6 in %s on line 18

Warning: Undefined variable $u7 in %s on line 18

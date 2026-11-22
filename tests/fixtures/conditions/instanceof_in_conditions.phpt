--TEST--
instanceof_in_conditions
--FILE--
<?php
interface I {}
class A implements I {}
class B extends A {}
class C {}
$o = new B();
$n = new C();
if ($o instanceof I) echo "i\n";
if ($o instanceof A) echo "a\n";
if ($o instanceof B) echo "b\n"; else echo "nb\n";
if ($o instanceof C) echo "c\n"; else echo "nc\n";
if (!($o instanceof C)) echo "notc\n";
if ($n instanceof A) echo "na\n"; else echo "notna\n";
while ($n instanceof A) { echo "never\n"; break; }
do { echo "once\n"; } while ($n instanceof A);
$cls = 'A';
if ($o instanceof $cls) echo "dyn-a\n";
$x = $n instanceof $cls ? 'y' : 'n';
echo $x, "\n";
$x2 = !($o instanceof $cls) ? 'y' : 'n';
echo $x2, "\n";
$obj = new C();
if ($o instanceof $obj) echo "obj-yes\n"; else echo "obj-no\n";
var_dump($o instanceof I, $n instanceof A, 5 instanceof A, null instanceof A);
function pick($v, $c) { return $v instanceof $c ? 'yes' : 'no'; }
echo pick($o, 'B'), pick($n, 'B'), "\n";
class D { public function selfCheck() { return $this instanceof D; } }
echo (new D())->selfCheck() ? "self\n" : "notself\n";
--EXPECTF--
i
a
b
nc
notc
notna
once
dyn-a
n
n
obj-no
bool(true)
bool(false)
bool(false)
bool(false)
yesno
self

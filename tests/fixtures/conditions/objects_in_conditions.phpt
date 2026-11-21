--TEST--
objects_in_conditions
--FILE--
<?php
class S { public function __construct(public string $s) {} public function __toString(): string { echo "[toString]"; return $this->s; } }
class Plain {}
$a = new S("5"); $b = new S("abc");
if ($a == "5") echo "eq5\n";
if ($a < "6") echo "lt6\n";
if ($a === $a) echo "self\n";
if ($a == $b) echo "ab-eq\n"; else echo "ab-ne\n";
try { if ($a < new Plain) echo "x\n"; } catch (Error $e) { echo "Error: ", $e->getMessage(), "\n"; }
try { var_dump($a < new Plain); } catch (Error $e) { echo "Error2: ", $e->getMessage(), "\n"; }
$o = new stdClass;
if ($o == $o) echo "obj-eq\n";
if ($o === $o) echo "obj-ident\n";
$c = new S("5"); $d = $c;
if ($c === $d) echo "clone-ident\n";
--EXPECTF--
[toString]eq5
[toString]lt6
self
ab-ne
bool(false)
obj-eq
obj-ident
clone-ident

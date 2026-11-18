--TEST--
A private setter runs in class scope and rejects global writes
--FILE--
<?php
class C {
    public private(set) int $x = 1 { set => $value + 1; }
    public function change(int $value) { $this->x = $value; }
}
$c = new C;
$c->change(3);
echo $c->x, "\n";
try { $c->x = 5; } catch (Error $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
4
Cannot modify private(set) property C::$x from global scope

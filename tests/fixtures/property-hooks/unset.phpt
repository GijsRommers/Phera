--TEST--
Unsetting a hooked property throws without calling magic
--FILE--
<?php
class C {
    public $x = 1 { get => $this->x; }
    public function __unset($name) { echo "magic\n"; }
}
$c = new C;
try { unset($c->x); } catch (Error $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
Cannot unset hooked property C::$x

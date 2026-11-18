--TEST--
traits: private final method does not prevent shadowing
--FILE--
<?php
trait T {
    private final function f() {}
}
class C { use T; }
class D extends C { public function f() { echo "D"; } }
(new D)->f();
--EXPECTF--
Warning: Private methods cannot be final as they are never overridden by other classes in %s on line 3
D

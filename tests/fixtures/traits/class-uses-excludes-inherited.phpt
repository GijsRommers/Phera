--TEST--
traits: class uses excludes inherited
--FILE--
<?php
trait T{} class P{use T;} class C extends P{} var_export(class_uses(C::class));
--EXPECT--
array (
)

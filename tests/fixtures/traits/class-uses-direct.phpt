--TEST--
traits: class uses direct
--FILE--
<?php
trait T{} trait U{use T;} class C{use U;} var_export(class_uses(C::class));
--EXPECT--
array (
  'U' => 'U',
)

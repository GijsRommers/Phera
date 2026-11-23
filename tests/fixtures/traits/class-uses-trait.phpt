--TEST--
traits: class uses trait
--FILE--
<?php
trait T{} trait U{use T;} var_export(class_uses(U::class));
--EXPECT--
array (
  'T' => 'T',
)

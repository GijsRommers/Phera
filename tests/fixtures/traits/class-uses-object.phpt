--TEST--
traits: class uses object
--FILE--
<?php
trait T{} class C{use T;} var_export(class_uses(new C));
--EXPECT--
array (
  'T' => 'T',
)

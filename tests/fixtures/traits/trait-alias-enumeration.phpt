--TEST--
traits: trait alias enumeration
--FILE--
<?php
trait PrimaryTrait {}
class_alias(PrimaryTrait::class, "SecondaryTrait");
var_export(get_declared_traits());
--EXPECT--
array (
  0 => 'PrimaryTrait',
  1 => 'secondarytrait',
)

--TEST--
traits: trait alias keeps identity
--FILE--
<?php
trait OriginalTrait { function answer() { return __TRAIT__; } }
class_alias(OriginalTrait::class, "AlternateTrait");
class Consumer {use AlternateTrait;}
var_dump(trait_exists("alternatetrait"), (new Consumer)->answer());
var_export(class_uses(Consumer::class));
--EXPECT--
bool(true)
string(13) "OriginalTrait"
array (
  'OriginalTrait' => 'OriginalTrait',
)

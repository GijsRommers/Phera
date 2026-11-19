--TEST--
Serialization initializes a ghost unless its creation opted out
--FILE--
<?php
class Payload { public int $value = 1; }
function initialize(Payload $object) { $object->value = 42; }
$reflector = new ReflectionClass(Payload::class);
$ordinary = $reflector->newLazyGhost('initialize');
$skip = $reflector->newLazyGhost('initialize', ReflectionClass::SKIP_INITIALIZATION_ON_SERIALIZE);
var_dump(serialize($ordinary), $reflector->isUninitializedLazyObject($ordinary));
var_dump(serialize($skip), $reflector->isUninitializedLazyObject($skip));
--EXPECT--
string(35) "O:7:"Payload":1:{s:5:"value";i:42;}"
bool(false)
string(18) "O:7:"Payload":0:{}"
bool(true)

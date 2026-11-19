--TEST--
Serialization uses initialized slots when the skip option is set
--FILE--
<?php
class Entry { public int $id; public string $label; }
$class = new ReflectionClass(Entry::class);
$entry = $class->newLazyProxy(static function () {
    echo "factory
";
    return new Entry();
}, ReflectionClass::SKIP_INITIALIZATION_ON_SERIALIZE);
$class->getProperty('id')->setRawValueWithoutLazyInitialization($entry, 7);
echo serialize($entry), "
";
var_dump($class->isUninitializedLazyObject($entry));
--EXPECT--
O:5:"Entry":1:{s:2:"id";i:7;}
bool(true)

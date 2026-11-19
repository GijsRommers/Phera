--TEST--
Native reflection initializes ghost and proxy properties and preserves exception frames
--FILE--
<?php
class NativeLazyEntry {
    public string $name;
    public function __construct(string $name) { $this->name = $name; }
}
$class = new ReflectionClass(NativeLazyEntry::class);
$property = $class->getProperty('name');
$ghost = $class->newLazyGhost(function (NativeLazyEntry $object) {
    echo "ghost\n";
    $object->name = 'first';
});
$proxy = $class->newLazyProxy(function () {
    echo "proxy\n";
    return new NativeLazyEntry('second');
});
var_dump($class->isUninitializedLazyObject($ghost), $class->isUninitializedLazyObject($proxy));
var_dump($property->getValue($ghost), $property->getRawValue($proxy));
var_dump($class->isUninitializedLazyObject($ghost), $class->isUninitializedLazyObject($proxy));
$failing = $class->newLazyGhost(function () { throw new RuntimeException('initializer failed'); });
try {
    $property->getValue($failing);
} catch (RuntimeException $error) {
    $frames = $error->getTrace();
    echo $error->getMessage(), "\n";
    foreach ($frames as $frame) {
        if (($frame['class'] ?? '') === ReflectionProperty::class) {
            echo $frame['class'], $frame['type'], $frame['function'], ':', $frame['line'], "\n";
        }
    }
}
--EXPECT--
bool(true)
bool(true)
ghost
proxy
string(5) "first"
string(6) "second"
bool(false)
bool(false)
initializer failed
ReflectionProperty->getValue:21

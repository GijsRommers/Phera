--TEST--
Raw property reflection bypasses hooks and resolves the backing slot on subclasses
--FILE--
<?php
class Slot {
    public int $number {
        get => $this->number + 1;
        set => $value * 2;
    }
    public int $virtual { get => 5; }
    public int $writeOnly { set {} }
}
class ExtendedSlot extends Slot {
    public int $virtual {
        get => $this->virtual + 10;
        set => $value * 2;
    }
}
$slot = new Slot();
$property = new ReflectionProperty(Slot::class, 'number');
var_dump($property->isInitialized($slot));
try {
    $property->getRawValue($slot);
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
$property->setValue($slot, '3');
var_dump($property->getRawValue($slot), $property->getValue($slot));
$property->setRawValue($slot, '10');
var_dump($property->getRawValue($slot), $property->getValue($slot));
foreach (['virtual', 'writeOnly'] as $name) {
    $property = new ReflectionProperty(Slot::class, $name);
    var_dump($property->isInitialized($slot));
    try {
        echo $property->getRawValue($slot), "\n";
    } catch (Error $e) {
        echo $e->getMessage(), "\n";
    }
    try {
        $property->setRawValue($slot, 1);
    } catch (Error $e) {
        echo $e->getMessage(), "\n";
    }
}
$derived = new ExtendedSlot();
$virtual = new ReflectionProperty(Slot::class, 'virtual');
$virtual->setRawValue($derived, 7);
var_dump($virtual->getRawValue($derived), $virtual->getValue($derived));
--EXPECT--
bool(false)
Typed property Slot::$number must not be accessed before initialization
int(6)
int(7)
int(10)
int(11)
bool(true)
Must not read from virtual property Slot::$virtual
Property Slot::$virtual is read-only
bool(true)
Property Slot::$writeOnly is write-only
Must not write to virtual property Slot::$writeOnly
int(7)
int(17)

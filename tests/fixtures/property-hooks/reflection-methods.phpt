--TEST--
Property hooks reflect as exact method declarations and have typed enum selectors
--FILE--
<?php
interface Measured {
    public int $reading {
        get;
    }
}
class Gauge implements Measured {
    public int $reading = 2 {
        get => $this->reading + 1;
        final set(int|string $reading) => (int) $reading * 2;
    }
}
class ChildGauge extends Gauge {
    public int $reading = 8 {
        get => parent::$reading::get() + 10;
    }
    final public int $virtual {
        get => 9;
    }
}
$class = new ReflectionEnum(PropertyHookType::class);
echo $class->getName(), ': ', implode(', ', array_map(fn($e) => $e->name, PropertyHookType::cases())), "\n";
$parent = new ReflectionProperty(Gauge::class, 'reading');
$child = new ReflectionProperty(ChildGauge::class, 'reading');
foreach ([$parent, $child, new ReflectionProperty(Measured::class, 'reading'), new ReflectionProperty(ChildGauge::class, 'virtual')] as $property) {
    echo $property->class, '::$', $property->name, ' ',
        (int) $property->isAbstract(), (int) $property->isFinal(), (int) $property->isVirtual(), (int) $property->hasHooks(), ' ',
        (string) $property->getSettableType(), "\n";
    foreach (PropertyHookType::cases() as $kind) {
        $hook = $property->getHook($kind);
        echo $kind->name, ' ', (int) $property->hasHook($kind);
        if ($hook) {
            echo ' ', get_class($hook), ' ', $hook->class, '::', $hook->getName(), ' ', (string) $hook->getReturnType(), ' ', (int) $hook->isFinal();
        }
        echo "\n";
    }
    echo implode(',', array_keys($property->getHooks())), "\n";
}
$gauge = new ChildGauge();
$setter = $child->getHook(PropertyHookType::Set);
$setter->invokeArgs($gauge, ['reading' => '6']);
echo $parent->getHook(PropertyHookType::Get)->invoke($gauge), "\n";
echo $child->getHook(PropertyHookType::Get)->getClosure($gauge)(), "\n";
echo $child->getHook(PropertyHookType::Get)->getPrototype()->class, "\n";
--EXPECT--
PropertyHookType: Get, Set
Gauge::$reading 0001 string|int
Get 1 ReflectionMethod Gauge::$reading::get int 0
Set 1 ReflectionMethod Gauge::$reading::set void 1
get,set
ChildGauge::$reading 0001 string|int
Get 1 ReflectionMethod ChildGauge::$reading::get int 0
Set 1 ReflectionMethod Gauge::$reading::set void 1
get,set
Measured::$reading 1011 never
Get 1 ReflectionMethod Measured::$reading::get int 0
Set 0
get
ChildGauge::$virtual 0111 never
Get 1 ReflectionMethod ChildGauge::$virtual::get int 0
Set 0
get
13
23
Measured

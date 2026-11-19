--TEST--
A proxy factory must return an instance of a compatible class
--FILE--
<?php
class Item { public int $quantity = 3; }
$class = new ReflectionClass(Item::class);
foreach ([null, new stdClass()] as $result) {
    $proxy = $class->newLazyProxy(static fn () => $result);
    try {
        $class->initializeLazyObject($proxy);
    } catch (TypeError $error) {
        echo $error->getMessage(), "
";
    }
}
--EXPECT--
Lazy proxy factory must return an instance of a class compatible with Item, null returned
The real instance class stdClass is not compatible with the proxy class Item. The proxy must be a instance of the same class as the real instance, or a sub-class with no additional properties, and no overrides of the __destructor or __clone methods.

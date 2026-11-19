--TEST--
Ghost initializer must return null or no value
--FILE--
<?php
class Item { public int $count = 3; }
$class = new ReflectionClass(Item::class);
$item = $class->newLazyGhost(fn (Item $self) => 4);
try {
    $class->initializeLazyObject($item);
} catch (TypeError $error) {
    echo $error->getMessage(), "
";
}
var_dump($class->isUninitializedLazyObject($item));
--EXPECT--
Lazy object initializer must return NULL or no value
bool(true)

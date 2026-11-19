--TEST--
ReflectionClass exposes Generator as a final Core iterator with native method signatures
--FILE--
<?php
$class = new ReflectionClass(Generator::class);
echo $class->getName(), ' ', $class->getExtensionName(), "
";
var_dump($class->isInternal(), $class->isFinal(), $class->isIterable(),
         $class->isInstantiable(), $class->isCloneable(), $class->getConstructor());
echo implode(',', $class->getInterfaceNames()), "
";
foreach ($class->getMethods() as $method) {
    echo $method->getName();
    foreach ($method->getParameters() as $parameter) {
        echo ' ', $parameter->getType(), ' $', $parameter->getName();
    }
    echo ' -> ', $method->getReturnType(), ' ', $method->getExtensionName(), "
";
}
foreach (['newInstance', 'newInstanceWithoutConstructor'] as $instantiate) {
    try { $class->$instantiate(); }
    catch (Throwable $error) { echo get_class($error), ' ', $error->getMessage(), "
"; }
}
--EXPECT--
Generator Core
bool(true)
bool(true)
bool(true)
bool(true)
bool(false)
NULL
Iterator,Traversable
rewind -> void Core
valid -> bool Core
current -> mixed Core
key -> mixed Core
next -> void Core
send mixed $value -> mixed Core
throw Throwable $exception -> mixed Core
getReturn -> mixed Core
__debugInfo -> array Core
Error The "Generator" class is reserved for internal use and cannot be manually instantiated
ReflectionException Class Generator is an internal class marked as final that cannot be instantiated without invoking its constructor

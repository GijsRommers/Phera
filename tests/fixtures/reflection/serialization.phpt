--TEST--
Reflection serialization follows PHP and preserves reflected private writes
--FILE--
<?php
declare(strict_types=1);

#[Attribute]
class Marker {}

class Record {
    public const VERSION = 1;
    private int $count = 0;
    public function handle(Countable&Iterator $value): int|string { return 1; }
}

enum Choice: int { case One = 1; }

$record = new Record();
$class = new ReflectionClass($record);
$method = $class->getMethod('handle');
$parameter = $method->getParameters()[0];
$property = $class->getProperty('count');
$case = new ReflectionEnumBackedCase(Choice::class, 'One');
$values = [&$record];
$reflectors = [
    $class,
    new ReflectionObject($record),
    new ReflectionFunction('strlen'),
    $method,
    $property,
    new ReflectionClassConstant(Record::class, 'VERSION'),
    $parameter,
    $property->getType(),
    $method->getReturnType(),
    $parameter->getType(),
    new ReflectionExtension('Reflection'),
    ReflectionReference::fromArrayElement($values, 0),
    (new ReflectionClass(Marker::class))->getAttributes()[0],
    $case->getEnum(),
    new ReflectionEnumUnitCase(Choice::class, 'One'),
    $case,
    new ReflectionConstant('PHP_VERSION'),
];
foreach ($reflectors as $reflector) {
    try {
        serialize($reflector);
        echo 'Serialized ', get_class($reflector), "\n";
    } catch (Throwable $error) {
        echo get_class($error), ': ', $error->getMessage(), "\n";
    }
}
echo serialize(new Reflection()), "\n";
var_dump(str_starts_with(serialize(new ReflectionException()), 'O:19:"ReflectionException":'));
$property->setValue($record, '42');
echo bin2hex(serialize($record)), "\n";
--EXPECT--
Exception: Serialization of 'ReflectionClass' is not allowed
Exception: Serialization of 'ReflectionObject' is not allowed
Exception: Serialization of 'ReflectionFunction' is not allowed
Exception: Serialization of 'ReflectionMethod' is not allowed
Exception: Serialization of 'ReflectionProperty' is not allowed
Exception: Serialization of 'ReflectionClassConstant' is not allowed
Exception: Serialization of 'ReflectionParameter' is not allowed
Exception: Serialization of 'ReflectionNamedType' is not allowed
Exception: Serialization of 'ReflectionUnionType' is not allowed
Exception: Serialization of 'ReflectionIntersectionType' is not allowed
Exception: Serialization of 'ReflectionExtension' is not allowed
Exception: Serialization of 'ReflectionReference' is not allowed
Exception: Serialization of 'ReflectionAttribute' is not allowed
Exception: Serialization of 'ReflectionEnum' is not allowed
Exception: Serialization of 'ReflectionEnumUnitCase' is not allowed
Exception: Serialization of 'ReflectionEnumBackedCase' is not allowed
Exception: Serialization of 'ReflectionConstant' is not allowed
O:10:"Reflection":0:{}
bool(true)
4f3a363a225265636f7264223a313a7b733a31333a22005265636f726400636f756e74223b693a34323b7d

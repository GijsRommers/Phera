--TEST--
Reflection constructors use linked targets and autoload missing classes in order
--FILE--
<?php
error_reporting(E_ALL);
set_error_handler(function (int $errno, string $message): bool {
    echo "$errno: $message\n";
    return true;
});
spl_autoload_register(function (string $name): void {
    echo "first: $name\n";
});
spl_autoload_register(function (string $name): void {
    echo "second: $name\n";
    if ($name === 'LoadedDuringReflection') {
        #[AllowDynamicProperties]
        class LoadedDuringReflection {
            private const VERSION = 4;
            private string $secret = 'hidden';
            protected static function act(): string { return static::class; }
        }
    }
    if ($name === 'StatusDuringReflection') {
        enum StatusDuringReflection: string { case Ready = 'ready'; }
    }
});

$class = new ReflectionClass('\\LoadedDuringReflection');
class_alias($class->name, 'ReflectionAlias');
$aliased = new ReflectionClass('REFLECTIONALIAS');
$object = new LoadedDuringReflection();
$object->extra = 9;
echo $class->name, ' ', $aliased->name, ' ', (new ReflectionClass($object))->name, "\n";
echo (new ReflectionObject($object))->getProperty('extra')->getValue($object), "\n";

class ReflectionChild extends LoadedDuringReflection {}
$method = new ReflectionMethod('ReflectionChild', 'ACT');
echo $method->class, ' ', $method->name, ' ', $method->invoke(null), "\n";
echo (new ReflectionMethod($object, 'act'))->name, "\n";
echo (new ReflectionMethod('LoadedDuringReflection::act'))->name, "\n";
echo (new ReflectionMethod(method: 'act', objectOrMethod: $object))->name, "\n";
echo (new ReflectionProperty($object, 'secret'))->getValue($object), "\n";
echo (new ReflectionClassConstant('ReflectionAlias', 'VERSION'))->getValue(), "\n";

$enum = new ReflectionEnum('StatusDuringReflection');
echo (new ReflectionEnum(StatusDuringReflection::Ready))->name, ' ', $enum->getBackingType(), "\n";
echo (new ReflectionFunction('STRLEN'))->name, "\n";
$closure = static fn (int $value): int => $value + 1;
echo (new ReflectionFunction($closure))->invoke(6), ' ', (new ReflectionMethod($closure, '__invoke'))->invoke($closure, 7), "\n";

foreach ([
    fn () => new ReflectionClass('MissingDuringReflection'),
    fn () => new ReflectionMethod($object, 'missing'),
    fn () => new ReflectionProperty($object, 'missing'),
    fn () => new ReflectionClassConstant($object, 'MISSING'),
    fn () => new ReflectionEnum($object),
    fn () => new ReflectionFunction('missing_during_reflection'),
] as $make) {
    try {
        $make();
    } catch (ReflectionException $error) {
        echo $error::class, ': ', $error->getMessage(), "\n";
    }
}
--EXPECT--
first: LoadedDuringReflection
second: LoadedDuringReflection
LoadedDuringReflection LoadedDuringReflection LoadedDuringReflection
9
LoadedDuringReflection act ReflectionChild
act
8192: Calling ReflectionMethod::__construct() with 1 argument is deprecated, use ReflectionMethod::createFromMethodName() instead
act
act
hidden
4
first: StatusDuringReflection
second: StatusDuringReflection
StatusDuringReflection string
strlen
7 8
first: MissingDuringReflection
second: MissingDuringReflection
ReflectionException: Class "MissingDuringReflection" does not exist
ReflectionException: Method LoadedDuringReflection::missing() does not exist
ReflectionException: Property LoadedDuringReflection::$missing does not exist
ReflectionException: Constant LoadedDuringReflection::MISSING does not exist
ReflectionException: Class "LoadedDuringReflection" is not an enum
ReflectionException: Function missing_during_reflection() does not exist

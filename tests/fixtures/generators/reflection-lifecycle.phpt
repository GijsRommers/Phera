--TEST--
ReflectionGenerator describes the selected activation and still exposes its function after it closes
--FILE--
<?php
class Source {
    function values(): Generator {
        yield 1;
        yield 2;
    }
}
$source = new Source;
$generator = $source->values();
$reflection = new ReflectionGenerator($generator);
function inspectGenerator(ReflectionGenerator $reflection): void {
    var_dump($reflection->isClosed(), $reflection->getExecutingFile() === __FILE__);
    echo $reflection->getExecutingLine(), "
";
}
inspectGenerator($reflection);
var_dump($reflection->getThis() === $source, $reflection->getExecutingGenerator() === $generator);
$generator->current();
inspectGenerator($reflection);
$generator->next();
inspectGenerator($reflection);
$generator->next();
var_dump($reflection->isClosed());
$method = $reflection->getFunction();
echo get_class($method), ' ', $method->getDeclaringClass()->getName(), '::', $method->getName(), "
";
foreach (['getExecutingLine', 'getExecutingFile', 'getThis', 'getExecutingGenerator', 'getTrace'] as $name) {
    try {
        $reflection->$name();
    } catch (ReflectionException $exception) {
        echo "$name ", $exception->getMessage(), "
";
    }
}
--EXPECT--
bool(false)
bool(true)
3
bool(true)
bool(true)
bool(false)
bool(true)
4
bool(false)
bool(true)
5
bool(true)
ReflectionMethod Source::values
getExecutingLine Cannot fetch information from a closed Generator
getExecutingFile Cannot fetch information from a closed Generator
getThis Cannot fetch information from a closed Generator
getExecutingGenerator Cannot fetch information from a closed Generator
getTrace Cannot fetch information from a closed Generator

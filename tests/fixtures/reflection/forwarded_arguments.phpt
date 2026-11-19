--TEST--
Reflective invocation forwards argument names and reports uncallable constructors
--FILE--
<?php
function joinNames(string $first, string $last): string {
    return $first . ' ' . $last;
}
class Person {
    public function __construct(public string $first = 'a', public string $last = 'b') {}
    public function show(string $separator = '-'): string {
        return $this->first . $separator . $this->last;
    }
}
$class = new ReflectionClass(Person::class);
$person = $class->newInstance(last: 'Bond', first: 'James');
$method = $class->getMethod('show');
echo $method->invoke($person, separator: ':'), "\n";
echo $method->invokeArgs($person, ['separator' => '/']), "\n";
$function = new ReflectionFunction('joinNames');
echo $function->invoke(last: 'Doe', first: 'Jane'), "\n";
try {
    $function->invoke(invalid: 'name');
} catch (Error $exception) {
    echo $exception->getMessage(), "\n";
}
$closure = new ReflectionClass(Closure::class);
var_dump($closure->getConstructor()->isPrivate());
try {
    $closure->newInstance(1);
} catch (ReflectionException $exception) {
    echo $exception->getMessage(), "\n";
}
--EXPECT--
James:Bond
James/Bond
Jane Doe
Unknown named parameter $invalid
bool(true)
Class Closure does not have a constructor, so you cannot pass any constructor arguments

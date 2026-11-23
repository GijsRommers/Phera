--TEST--
Attributed functions and methods produce a Generator and retain their metadata
--FILE--
<?php
#[Attribute]
class Label {
    function __construct(public string $value) {}
}
#[Label('function')]
function numbers(): Generator {
    echo "function body
";
    yield 7;
}
class Source {
    #[Label('method')]
    function numbers(): Generator {
        echo "method body
";
        yield 8;
    }

    function ordinary(): int { return 9; }
}
$source = new Source();
$function = new ReflectionFunction('numbers');
$method = new ReflectionMethod(Source::class, 'numbers');
foreach ([$function, $method, new ReflectionMethod(Source::class, 'ordinary'), new ReflectionFunction('strlen')] as $reflection) {
    var_dump($reflection->isGenerator());
    foreach ($reflection->getAttributes(Label::class) as $attribute) {
        echo $attribute->newInstance()->value, "
";
    }
}
$first = $function->invoke();
$second = $method->invoke($source);
echo "created
";
var_dump($first->current(), $second->current());
--EXPECT--
bool(true)
function
bool(true)
method
bool(false)
bool(false)
created
function body
method body
int(7)
int(8)

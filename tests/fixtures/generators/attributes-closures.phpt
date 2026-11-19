--TEST--
Attributed generator closures and arrows retain attributes and their generator identity
--FILE--
<?php
#[Attribute]
class Marker {
    function __construct(public int $value) {}
}
$seed = 4;
$closure = #[Marker(10)] function () use ($seed): Generator { yield $seed; };
$arrow = #[Marker(11)] fn(): Generator => yield $seed + 1;
foreach ([$closure, $arrow] as $callable) {
    $function = new ReflectionFunction($callable);
    $generator = $callable();
    $reflection = new ReflectionGenerator($generator);
    foreach ([$function, $reflection->getFunction()] as $observed) {
        var_dump($observed->isGenerator(), $observed->getClosure() === $callable);
        echo $observed->getAttributes(Marker::class)[0]->newInstance()->value, "
";
    }
    var_dump($generator->current());
    $generator->next();
    var_dump($reflection->getFunction()->getClosure() === $callable);
}
--EXPECT--
bool(true)
bool(true)
10
bool(true)
bool(true)
10
int(4)
bool(true)
bool(true)
bool(true)
11
bool(true)
bool(true)
11
int(5)
bool(true)

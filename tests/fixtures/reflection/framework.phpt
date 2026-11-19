--TEST--
Reflection autowires services, hydrates mapped properties and runs attributed tests
--FILE--
<?php
#[Attribute(Attribute::TARGET_PROPERTY)]
class Column {
    public function __construct(public string $name, public string $type) {}
}
#[Attribute(Attribute::TARGET_METHOD)]
class Test {}
#[Attribute(Attribute::TARGET_METHOD)]
class DataProvider {
    public function __construct(public string $method) {}
}

class Clock {
    public function now(): int { return 42; }
}
class Service {
    public function __construct(public Clock $clock) {}
}
class Entity {
    #[Column(name: 'user_id', type: 'int')]
    private int $id;
    public function id(): int { return $this->id; }
}
class Suite {
    public function data(): array { return [[2, 4], [3, 9]]; }

    #[Test, DataProvider('data')]
    private function square(int $value, int $expected): void {
        echo $value * $value === $expected ? "pass\n" : "fail\n";
    }
}

function wire(string $name): object {
    $class = new ReflectionClass($name);
    $args = [];
    if ($method = $class->getConstructor()) {
        foreach ($method->getParameters() as $parameter) {
            $args[$parameter->getName()] = wire($parameter->getType()->getName());
        }
    }
    return $class->newInstanceArgs($args);
}
function hydrate(string $name, array $row): object {
    $class = new ReflectionClass($name);
    $entity = $class->newInstanceWithoutConstructor();
    foreach ($class->getProperties() as $property) {
        foreach ($property->getAttributes(Column::class) as $attribute) {
            $column = $attribute->newInstance();
            $property->setValue($entity, $row[$column->name]);
            echo $column->name, " ", $column->type, "\n";
        }
    }
    return $entity;
}
function runTests(object $suite): void {
    $class = new ReflectionObject($suite);
    foreach ($class->getMethods() as $method) {
        if ($method->getAttributes(Test::class)) {
            $provider = $method->getAttributes(DataProvider::class)[0]->newInstance();
            foreach ($class->getMethod($provider->method)->invoke($suite) as $args) {
                $method->invokeArgs($suite, $args);
            }
        }
    }
}
echo (int) extension_loaded('Reflection'), "\n";
echo wire(Service::class)->clock->now(), "\n";
echo hydrate(Entity::class, ['user_id' => '19'])->id(), "\n";
runTests(new Suite());
--EXPECT--
1
42
user_id int
19
pass
pass

--TEST--
Compiled container returns a ghost whose typed property loads on access
--FILE--
<?php
class Service {
    public string $name;
}
class Container {
    public array $privates = [];
    public function service(): Service {
        $reflector = new ReflectionClass(Service::class);
        return $this->privates['x'] = $reflector->newLazyGhost(static function (Service $ghost) {
            echo "loaded
";
            $ghost->name = 'gijs';
        });
    }
}
$container = new Container();
$service = $container->service();
var_dump($container->privates['x'] === $service);
echo $service->name, "
";
echo $service->name, "
";
--EXPECT--
bool(true)
loaded
gijs
gijs

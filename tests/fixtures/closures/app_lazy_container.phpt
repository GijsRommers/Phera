--TEST--
app lazy container
--FILE--
<?php
class Container {
    private $factories = [];
    private $instances = [];
    public function register($name, callable $factory) {
        $this->factories[$name] = $factory;
    }
    public function get($name) {
        if (isset($this->instances[$name])) {
            return $this->instances[$name];
        }
        if (!isset($this->factories[$name])) {
            throw new RuntimeException('missing:' . $name);
        }
        $factory = $this->factories[$name];
        $instance = $factory($this);
        $this->instances[$name] = $instance;
        return $instance;
    }
}
class Greeting {
    public function __construct(public $prefix) {}
    public function say($name) {
        return $this->prefix . strtoupper($name);
    }
}
$container = new Container;
$created = 0;
$container->register('prefix', fn($container) => 'hello:');
$container->register('greeting', function ($container) use (&$created) {
    ++$created;
    return new Greeting($container->get('prefix'));
});
$a = $container->get('greeting');
$b = $container->get('greeting');
echo $a->say('ada'), "\n";
var_dump($a === $b, $created);
try {
    $container->get('unknown');
} catch (RuntimeException $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
hello:ADA
bool(true)
int(1)
missing:unknown

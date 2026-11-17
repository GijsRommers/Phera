--TEST--
app router
--FILE--
<?php
class Router {
    private $routes = [];
    public function add($path, callable $handler) {
        $this->routes[$path] = $handler;
    }
    public function dispatch($path, $arguments) {
        if (!isset($this->routes[$path])) {
            throw new RuntimeException('not found');
        }
        return call_user_func_array($this->routes[$path], $arguments);
    }
}
class Controller {
    public function __construct(private $prefix) {}
    public function show($name) {
        return $this->prefix . strtoupper($name);
    }
}
$router = new Router;
$controller = new Controller('user:');
$router->add('/show', $controller->show(...));
$router->add('/length', strlen(...));
$router->add('/sum', fn($a, $b = 2) => $a + $b);
echo $router->dispatch('/show', ['ada']), "\n";
var_dump($router->dispatch('/length', ['hello']));
var_dump($router->dispatch('/sum', ['a' => 3]));
try {
    $router->dispatch('/missing', []);
} catch (RuntimeException $error) {
    echo $error->getMessage(), "\n";
}
$router->add('/show', fn($name) => strtolower($name));
echo $router->dispatch('/show', ['ADA']), "\n";
--EXPECT--
user:ADA
int(5)
int(5)
not found
ada

<?php
// Patterns that framework code runs on every request, inside methods as in
// a container, a config repository and a router: elements of array
// properties read and tested (`isset($this->bindings[$id])`,
// `$this->items['app']['name'] ?? null`), string comparison and
// concatenation, `??` on variables and array keys, `!`, `static::class`,
// protected calls between a class and its parent, and namespaced calls of
// built-in functions.
namespace App;

abstract class Repository {
    protected array $items = [];
    protected function has(string $key): bool { return isset($this->items[$key]); }
}

final class Config extends Repository {
    public function __construct() {
        $this->items = ['app' => ['name' => 'demo', 'debug' => false], 'cache' => ['store' => 'array']];
    }
    public function get(string $group, string $key, $default = null) {
        return $this->items[$group][$key] ?? $default;
    }
    public function enabled(string $group): bool { return $this->has($group) && !empty($this->items[$group]); }
}

final class Container {
    private array $bindings = [];
    private array $instances = [];
    public function bind(string $id, string $class): void { $this->bindings[$id] = $class; }
    public function has(string $id): bool { return isset($this->instances[$id]) || isset($this->bindings[$id]); }
    public function resolve(string $id): string {
        if (isset($this->instances[$id])) { return $this->instances[$id]; }
        $name = $this->bindings[$id] ?? $id;
        return $this->instances[$id] = $name . '@' . static::class;
    }
}

function route(array $routes, string $method, string $path): ?string {
    foreach ($routes as $route) {
        if ($route['method'] === $method && $route['path'] === $path) { return $route['name']; }
    }
    return null;
}

function run(int $rounds): string {
    $config = new Config;
    $container = new Container;
    $container->bind('cache', 'ArrayStore');
    $container->bind('config', 'Repository');
    $routes = [
        ['method' => 'GET', 'path' => '/api/users', 'name' => 'users'],
        ['method' => 'POST', 'path' => '/api/users', 'name' => 'users.store'],
        ['method' => 'GET', 'path' => '/api/hello', 'name' => 'hello'],
    ];
    $hits = 0; $length = 0; $missing = null;
    for ($i = 0; $i < $rounds; $i++) {
        $hits += $container->has('cache') ? 1 : 0;
        $hits += $container->has('nothing') ? 1 : 0;
        $length += strlen($container->resolve($i % 2 === 0 ? 'cache' : 'config'));
        $length += strlen($config->get('app', 'name', 'none'));
        $length += strlen((string) ($config->get('app', 'missing') ?? 'fallback'));
        $hits += $config->enabled('cache') ? 1 : 0;
        $hits += !$config->get('app', 'debug') ? 1 : 0;
        $name = route($routes, 'GET', '/api/hello') ?? 'none';
        $length += strlen('route:' . $name);
        $length += strlen($missing ?? 'default');
        $hits += str_starts_with($name, 'hel') ? 1 : 0;
    }
    return "$hits|$length";
}
echo run(200000), "\n";

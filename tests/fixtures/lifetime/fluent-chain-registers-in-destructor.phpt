--TEST--
fluent-chain-registers-in-destructor
--FILE--
<?php
// A configurator registers its service when it is destroyed, as Symfony's
// ServiceConfigurator does; the last one of a fluent chain is a temporary.
final class Registry { public static array $services = []; }
final class Configurator {
    private array $args = [];
    public function __construct(private Services $services, private string $id) {}
    public function args(array $args): static { $this->args = $args; return $this; }
    public function set(string $id): Configurator { return $this->services->set($id); }
    public function __destruct() { Registry::$services[$this->id] = $this->args; }
}
final class Services {
    public function set(string $id): Configurator { return new Configurator($this, $id); }
}
function registered(): string { return implode(',', array_keys(Registry::$services)); }

$services = new Services;
$services->set('a')->args([1])->set('b')->args([2]);
echo "next statement: ", registered(), "\n";

$configure = function (Services $services) {
    $services->set('c')->args([3])->set('d')->args([4]);
    return registered();
};
echo "inside the closure: ", $configure($services), "\n";
echo "after the closure: ", registered(), "\n";

for ($i = 0; $i < 3; $i++) {
    $services->set("loop$i")->args([$i]);
    echo "loop $i: ", count(Registry::$services), "\n";
}

function count_now(int $before, Configurator $configurator): int { return $before; }
echo "argument: ", count_now(count(Registry::$services), $services->set('e')->args([5])), "\n";
echo "after the call: ", registered(), "\n";
--EXPECT--
next statement: a,b
inside the closure: a,b,c,d
after the closure: a,b,c,d
loop 0: 5
loop 1: 6
loop 2: 7
argument: 7
after the call: a,b,c,d,loop0,loop1,loop2,e

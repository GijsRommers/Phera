--TEST--
long-fluent-chain-registers-in-destructor
--FILE--
<?php
// A fluent chain of more than 64 steps (Symfony's service files have them)
// inside a configuration closure: each configurator registers itself when
// it is destroyed, the last one at the end of the statement.
final class Registry { public static array $services = []; }
final class Configurator {
    private bool $public = false;
    private array $args = [];
    public function __construct(private Services $services, private string $id) {}
    public function public(): static { $this->public = true; return $this; }
    public function args(array $args): static { $this->args = $args; return $this; }
    public function set(string $id): Configurator { return $this->services->set($id); }
    public function __destruct() { Registry::$services[$this->id] = $this->args; }
}
final class Services {
    public function set(string $id): Configurator { return new Configurator($this, $id); }
}
$configure = static function (Services $services): void {
    $services
        ->set('s0')->public()->args([0])
        ->set('s1')->public()->args([1])
        ->set('s2')->public()->args([2])
        ->set('s3')->public()->args([3])
        ->set('s4')->public()->args([4])
        ->set('s5')->public()->args([5])
        ->set('s6')->public()->args([6])
        ->set('s7')->public()->args([7])
        ->set('s8')->public()->args([8])
        ->set('s9')->public()->args([9])
        ->set('s10')->public()->args([10])
        ->set('s11')->public()->args([11])
        ->set('s12')->public()->args([12])
        ->set('s13')->public()->args([13])
        ->set('s14')->public()->args([14])
        ->set('s15')->public()->args([15])
        ->set('s16')->public()->args([16])
        ->set('s17')->public()->args([17])
        ->set('s18')->public()->args([18])
        ->set('s19')->public()->args([19])
        ->set('s20')->public()->args([20])
        ->set('s21')->public()->args([21])
        ->set('s22')->public()->args([22])
        ->set('s23')->public()->args([23])
        ->set('s24')->public()->args([24])
        ->set('s25')->public()->args([25])
        ->set('s26')->public()->args([26])
        ->set('s27')->public()->args([27])
        ->set('s28')->public()->args([28])
        ->set('s29')->public()->args([29]);
    echo "before return: ", count(Registry::$services), "\n";
};
$configure(new Services);
echo count(Registry::$services), " ", implode(",", array_slice(array_keys(Registry::$services), -2)), "\n";
--EXPECT--
before return: 30
30 s28,s29

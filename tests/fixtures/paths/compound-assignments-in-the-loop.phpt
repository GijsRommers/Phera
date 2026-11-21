--TEST--
Compound assignments of properties and elements give the same results as the general path
--FILE--
<?php
class Counter
{
    public int $count = 0;
    public float $ratio = 1.0;
    public ?int $maybe = null;
    public $loose = '5';
    public array $sums = ['a' => 1, 'nested' => ['b' => 2]];
    public readonly int $fixed;
    public int $hooked = 0 { set => $value * 10; }
    public int $big = PHP_INT_MAX;
    public function __construct() { $this->fixed = 1; }
    public function bump(int $by): int { return $this->count += $by; }
}

class Magic
{
    private array $data = ['n' => 1];
    public function __get($name) { echo "get $name\n"; return $this->data[$name]; }
    public function __set($name, $value) { echo "set $name\n"; $this->data[$name] = $value; }
}

function run(): void
{
    $c = new Counter;
    $m = new Magic;
    for ($round = 0; $round < 3; $round++) {
        echo $c->bump(2), ' ';
        $c->ratio *= 1.5;
        $c->count -= 1;
        $c->count <<= 1;
        $c->count %= 1000;
        $c->loose += 1;
        $c->hooked += 1;
        $c->sums['a'] += $round;
        $c->sums['nested']['b'] *= 3;
        $c->sums['b'] ??= 0;
        try {
            $c->maybe += 1;
        } catch (TypeError $e) {
            echo $e->getMessage(), "\n";
        }
        try {
            $c->count /= 3;
        } catch (TypeError $e) {
            echo 'T: ', $e->getMessage(), "\n";
        }
        try {
            $c->fixed += 1;
        } catch (Error $e) {
            echo $e->getMessage(), "\n";
        }
        try {
            $c->big += 1;
        } catch (TypeError $e) {
            echo $e->getMessage(), "\n";
        }
        $m->n += 1;
        $a = ['k' => 1, 'f' => 0.5, 's' => '3', 'list' => [1, 2]];
        $copy = $a;
        $a['k'] += 2;
        $a['f'] -= 0.25;
        $a['s'] *= 2;
        $a['list'][0] |= 6;
        $a['k'] .= 'x';
        $a['missing'] = ($a['missing'] ?? 0) + 1;
        $kept = ($a['k2'] = 1) + ($a['k2'] += 4);
        try {
            $a['list'][1] += 'abc';
        } catch (TypeError $e) {
            echo $e->getMessage(), "\n";
        }
        $r = 1;
        $b = ['r' => &$r];
        $b['r'] += 10;
        $n = ['x' => null];
        $n['x'] += 5;
        $i = ['big' => PHP_INT_MAX];
        $i['big'] += 1;
        echo json_encode($a), json_encode($copy), $kept, ' ', $r, ' ', json_encode($n), ' ', var_export($i['big'], true), "\n";
        echo json_encode(get_object_vars($c)), "\n";
    }
    $u = [];
    $u['nope'] += 1;
    echo json_encode($u), "\n";
}

run();
--EXPECTF--
2 
Deprecated: Implicit conversion from float 0.6666666666666666 to int loses precision in %s on line 44
Cannot modify readonly property Counter::$fixed
Cannot assign float to property Counter::$big of type int
get n
set n
Unsupported operand types: int + string
{"k":"3x","f":0.25,"s":6,"list":[7,2],"missing":1,"k2":5}{"k":1,"f":0.5,"s":"3","list":[1,2]}6 11 {"x":5} 9.223372036854776E+18
{"count":0,"ratio":1.5,"maybe":1,"loose":6,"sums":{"a":1,"nested":{"b":6},"b":0},"fixed":1,"hooked":10,"big":9223372036854775807}
2 
Deprecated: Implicit conversion from float 0.6666666666666666 to int loses precision in %s on line 44
Cannot modify readonly property Counter::$fixed
Cannot assign float to property Counter::$big of type int
get n
set n
Unsupported operand types: int + string
{"k":"3x","f":0.25,"s":6,"list":[7,2],"missing":1,"k2":5}{"k":1,"f":0.5,"s":"3","list":[1,2]}6 11 {"x":5} 9.223372036854776E+18
{"count":0,"ratio":2.25,"maybe":2,"loose":7,"sums":{"a":2,"nested":{"b":18},"b":0},"fixed":1,"hooked":110,"big":9223372036854775807}
2 
Deprecated: Implicit conversion from float 0.6666666666666666 to int loses precision in %s on line 44
Cannot modify readonly property Counter::$fixed
Cannot assign float to property Counter::$big of type int
get n
set n
Unsupported operand types: int + string
{"k":"3x","f":0.25,"s":6,"list":[7,2],"missing":1,"k2":5}{"k":1,"f":0.5,"s":"3","list":[1,2]}6 11 {"x":5} 9.223372036854776E+18
{"count":0,"ratio":3.375,"maybe":3,"loose":8,"sums":{"a":4,"nested":{"b":54},"b":0},"fixed":1,"hooked":1110,"big":9223372036854775807}

Warning: Undefined array key "nope" in %s on line 84
{"nope":1}

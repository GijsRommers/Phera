--TEST--
A class loaded while a built-in evaluates a default or constant leaves the outer call intact
--FILE--
<?php
// Built-ins that evaluate a default or a constant needing a class from the
// autoloader, called while an outer call is waiting for that argument.
spl_autoload_register(function ($class) {
    echo "load $class\n";
    if (str_starts_with($class, 'Strategy')) {
        eval("class $class { const IMPLICIT = 1; const NONE = 2; }");
    }
});
class P1 { public int $s = Strategy1::IMPLICIT; }
class P2 { public int $s = Strategy2::IMPLICIT; public $list = []; }
class P3 { public int $s = Strategy3::NONE; public function __construct(public int $n = 0) {} }
class P4 { const VALUE = Strategy4::IMPLICIT; }
class P5 { public int $s = Strategy5::IMPLICIT; }
class P6 { public $s = Strategy6::NONE; }
class P7 { public $s = Strategy7::NONE; }
class P8 { const VALUE = Strategy8::NONE; }
class P9 { public $s = Strategy9::IMPLICIT; }

function outer($a, $b) { return (is_object($a) ? get_class($a) : var_export($a, true)) . ' ' . $b; }
function make($c) { return (new ReflectionClass($c))->newInstanceWithoutConstructor(); }
function hydrate(array $objects, array $properties) { return count($objects) . ' ' . get_class($objects[0]) . ' ' . json_encode($properties); }

echo outer(make('P1'), 'ok'), "\n";
echo hydrate($o = [(fn () => (new ReflectionClass('P2'))->newInstanceWithoutConstructor())()], ['list' => [1]]), "\n";
echo outer((new ReflectionClass('P3'))->newInstanceArgs([5]), 'args'), "\n";
echo outer(constant('P4::VALUE'), 'constant'), "\n";
echo outer(...array_map('make', ['P5']), ...['map']), "\n";
echo outer(unserialize('O:2:"P6":0:{}'), 'unserialize'), "\n";
echo outer((new ReflectionProperty('P7', 's'))->getDefaultValue(), 'default'), "\n";
echo outer((new ReflectionClassConstant('P8', 'VALUE'))->getValue(), 'class constant'), "\n";
echo outer(get_class_vars('P9')['s'], 'class vars'), "\n";
--EXPECT--
load Strategy1
P1 ok
load Strategy2
1 P2 {"list":[1]}
load Strategy3
P3 args
load Strategy4
1 constant
load Strategy5
P5 map
load Strategy6
P6 unserialize
load Strategy7
2 default
load Strategy8
2 class constant
load Strategy9
1 class vars

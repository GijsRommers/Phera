--TEST--
spl_object_id, strtolower, implode and integer to string, inside a namespace, qualified, shadowed by a function of the namespace, with wrong operands, lazy objects and PHP_INT_MIN
--FILE--
<?php
namespace App\Orm {
    function show(string $label, callable $f): void {
        try { $r = $f(); echo "$label: ", var_export($r, true), "\n"; }
        catch (\Throwable $e) { echo "$label: ", get_class($e), ': ', $e->getMessage(), ' @', count($e->getTrace()) ? $e->getTrace()[0]['function'] : '-', "\n"; }
    }
    set_error_handler(function ($no, $msg) { echo "  [$msg]\n"; return true; });
    class Entity { public $v = 1; function __toString(): string { return 'entity'; } }

    echo "-- spl_object_id\n";
    $a = new Entity; $b = new Entity;
    show('ids differ and count up', fn () => spl_object_id($b) - spl_object_id($a));
    show('same object twice', fn () => spl_object_id($a) === spl_object_id($a));
    show('qualified', fn () => \spl_object_id($a) === spl_object_id($a));
    show('closure', fn () => is_int(spl_object_id(fn () => 1)));
    show('freed id is reused', function () { $x = new Entity; $id = spl_object_id($x); unset($x); $y = new Entity; return $id === spl_object_id($y); });
    show('int', fn () => spl_object_id(5));
    show('string', fn () => spl_object_id('x'));
    show('null', fn () => spl_object_id(null));
    show('array', fn () => spl_object_id([]));
    show('no arguments', fn () => spl_object_id());
    $ghost = (new \ReflectionClass(Entity::class))->newLazyGhost(function ($o) { echo "init\n"; });
    show('lazy ghost keeps its id and stays lazy', fn () => [is_int(spl_object_id($ghost)), (new \ReflectionClass(Entity::class))->isUninitializedLazyObject($ghost)]);

    echo "-- strtolower\n";
    show('mixed', fn () => strtolower('Doctrine\\ORM\\EntityManager'));
    show('already lower', fn () => strtolower('abc'));
    show('empty', fn () => strtolower(''));
    show('bytes above ascii stay', fn () => bin2hex(strtolower("\xC4\xD6A\x00B")));
    show('int', fn () => strtolower(5));
    show('float', fn () => strtolower(1.5));
    show('bool', fn () => strtolower(true));
    show('null', fn () => strtolower(null));
    show('array', fn () => strtolower([]));
    show('object with __toString', fn () => strtolower(new Entity));
    show('shared string is not changed', function () { $s = 'ABC'; $t = $s; $u = strtolower($t); return [$s, $t, $u]; });
    show('strtolower keeps a lower string shared', function () { $s = 'abc'; $t = strtolower($s); $t .= 'd'; return [$s, $t]; });

    echo "-- implode\n";
    show('ints', fn () => implode(' ', [1, 2, 3]));
    show('negative and zero', fn () => implode(',', [-1, 0, 10, -100, 99, 100]));
    show('limits', fn () => implode(',', [PHP_INT_MAX, PHP_INT_MIN, PHP_INT_MIN + 1]));
    show('mixed scalars', fn () => implode('|', [1, 'a', true, false, null, 1.5, '']));
    show('one element', fn () => implode(',', [7]));
    show('empty', fn () => implode(',', []));
    show('array only', fn () => implode([1, 2]));
    show('keys are ignored', fn () => implode(',', ['a' => 1, 'b' => 2, 5 => 3]));
    show('long separator', fn () => implode(' :: ', [10, 20, 30, 40]));
    show('nested array', fn () => implode(',', [1, [2], 3]));
    show('object', fn () => implode(',', [new Entity, 2]));
    show('reference', function () { $x = 5; $a = [&$x, 6]; return implode(',', $a); });
    show('wrong', fn () => implode(',', 'x'));
    show('join', fn () => join('-', [1, 2, 3]));

    echo "-- integer to string\n";
    foreach ([0, 5, 9, 10, 99, 100, 12345, -1, -10, -99, -100, 1234567890123, PHP_INT_MAX, PHP_INT_MIN, PHP_INT_MIN + 1] as $n) {
        echo (string) $n, ' ', 'a' . $n, ' ', $n . 'a', ' ', $n . $n, ' ', "x{$n}y", ' ', strval($n), "\n";
    }
    show('int with float', fn () => 1 . 1.5);
    show('int with null', fn () => 12 . null);
    show('int with bool', fn () => 12 . true);
    show('int with array', fn () => 12 . []);
    $s = 'abc'; $s .= 12; $s .= -3;
    echo $s, "\n";
}
namespace App\Shadowed {
    function spl_object_id($o) { return 'mine'; }
    function strtolower($s) { return "lower($s)"; }
    echo "-- a function of the namespace wins\n";
    echo spl_object_id(new \stdClass), ' ', strtolower('ABC'), ' ', \strtolower('ABC'), "\n";
}
namespace {
    echo "-- global code\n";
    $o = new stdClass;
    echo spl_object_id($o) === spl_object_id($o) ? "same\n" : "different\n";
    echo strtolower('ÄBC'), "\n";
    try { strtolower(); } catch (\Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
    try { spl_object_id(1.5); } catch (\Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n", $e->getTraceAsString() === '#0 ' . __FILE__ . '(' . __LINE__ . '): spl_object_id(1.5)' . "\n" . '#1 {main}' ? "trace has the call\n" : "trace differs\n"; }
}

--EXPECT--
-- spl_object_id
ids differ and count up: 1
same object twice: true
qualified: true
closure: true
freed id is reused: true
int: TypeError: spl_object_id(): Argument #1 ($object) must be of type object, int given @spl_object_id
string: TypeError: spl_object_id(): Argument #1 ($object) must be of type object, string given @spl_object_id
null: TypeError: spl_object_id(): Argument #1 ($object) must be of type object, null given @spl_object_id
array: TypeError: spl_object_id(): Argument #1 ($object) must be of type object, array given @spl_object_id
no arguments: ArgumentCountError: spl_object_id() expects exactly 1 argument, 0 given @spl_object_id
lazy ghost keeps its id and stays lazy: array (
  0 => true,
  1 => true,
)
-- strtolower
mixed: 'doctrine\\orm\\entitymanager'
already lower: 'abc'
empty: ''
bytes above ascii stay: 'c4d6610062'
int: '5'
float: '1.5'
bool: '1'
  [strtolower(): Passing null to parameter #1 ($string) of type string is deprecated]
null: ''
array: TypeError: strtolower(): Argument #1 ($string) must be of type string, array given @strtolower
object with __toString: 'entity'
shared string is not changed: array (
  0 => 'ABC',
  1 => 'ABC',
  2 => 'abc',
)
strtolower keeps a lower string shared: array (
  0 => 'abc',
  1 => 'abcd',
)
-- implode
ints: '1 2 3'
negative and zero: '-1,0,10,-100,99,100'
limits: '9223372036854775807,-9223372036854775808,-9223372036854775807'
mixed scalars: '1|a|1|||1.5|'
one element: '7'
empty: ''
array only: '12'
keys are ignored: '1,2,3'
long separator: '10 :: 20 :: 30 :: 40'
  [Array to string conversion]
nested array: '1,Array,3'
object: 'entity,2'
reference: '5,6'
wrong: TypeError: implode(): Argument #2 ($array) must be of type ?array, string given @implode
join: '1-2-3'
-- integer to string
0 a0 0a 00 x0y 0
5 a5 5a 55 x5y 5
9 a9 9a 99 x9y 9
10 a10 10a 1010 x10y 10
99 a99 99a 9999 x99y 99
100 a100 100a 100100 x100y 100
12345 a12345 12345a 1234512345 x12345y 12345
-1 a-1 -1a -1-1 x-1y -1
-10 a-10 -10a -10-10 x-10y -10
-99 a-99 -99a -99-99 x-99y -99
-100 a-100 -100a -100-100 x-100y -100
1234567890123 a1234567890123 1234567890123a 12345678901231234567890123 x1234567890123y 1234567890123
9223372036854775807 a9223372036854775807 9223372036854775807a 92233720368547758079223372036854775807 x9223372036854775807y 9223372036854775807
-9223372036854775808 a-9223372036854775808 -9223372036854775808a -9223372036854775808-9223372036854775808 x-9223372036854775808y -9223372036854775808
-9223372036854775807 a-9223372036854775807 -9223372036854775807a -9223372036854775807-9223372036854775807 x-9223372036854775807y -9223372036854775807
int with float: '11.5'
int with null: '12'
int with bool: '121'
  [Array to string conversion]
int with array: '12Array'
abc12-3
-- a function of the namespace wins
mine lower(ABC) abc
-- global code
same
Äbc
ArgumentCountError: strtolower() expects exactly 1 argument, 0 given
TypeError: spl_object_id(): Argument #1 ($object) must be of type object, float given
trace has the call

--TEST--
Every guarded built-in in a namespace with no function of that name: the built-in's result, and its errors for wrong arguments. Each site runs twice, so that the second run takes the path the first one decided.
--FILE--
<?php
// Every guarded built-in in a namespace with no function of that name: the
// built-in's result, and its errors for wrong arguments. Each site runs
// twice, so that the second run takes the path the first one decided.
namespace App;
final class Box {}
const SOME = 1;
$values = [null, true, 0, 1.5, 'abc', '', [1, 2], new Box, STDIN];
for ($round = 0; $round < 2; $round++) {
    foreach ($values as $value) {
        echo json_encode([is_null($value), is_bool($value), is_int($value), is_integer($value), is_long($value),
            is_float($value), is_double($value), is_string($value), is_array($value), is_object($value),
            is_scalar($value), is_resource($value)]), ' ', gettype($value), "\n";
    }
    echo strlen('abcd'), ' ', count([1, 2, 3]), ' ', sizeof([1]), ' ', get_class(new Box), ' ';
    var_dump(defined('App\SOME'), defined('NOT_THERE'));
    foreach ([fn () => strlen([]), fn () => count(5), fn () => sizeof('x'), fn () => get_class(5), fn () => strlen(new Box)] as $call) {
        try { var_dump($call()); } catch (\TypeError $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
    }
    echo strlen(12345), ' ', strlen(1.5), ' ', strlen(true), "\n";
}
--EXPECT--
[true,false,false,false,false,false,false,false,false,false,false,false] NULL
[false,true,false,false,false,false,false,false,false,false,true,false] boolean
[false,false,true,true,true,false,false,false,false,false,true,false] integer
[false,false,false,false,false,true,true,false,false,false,true,false] double
[false,false,false,false,false,false,false,true,false,false,true,false] string
[false,false,false,false,false,false,false,true,false,false,true,false] string
[false,false,false,false,false,false,false,false,true,false,false,false] array
[false,false,false,false,false,false,false,false,false,true,false,false] object
[false,false,false,false,false,false,false,false,false,false,false,true] resource
4 3 1 App\Box bool(true)
bool(false)
TypeError: strlen(): Argument #1 ($string) must be of type string, array given
TypeError: count(): Argument #1 ($value) must be of type Countable|array, int given
TypeError: sizeof(): Argument #1 ($value) must be of type Countable|array, string given
TypeError: get_class(): Argument #1 ($object) must be of type object, int given
TypeError: strlen(): Argument #1 ($string) must be of type string, App\Box given
5 3 1
[true,false,false,false,false,false,false,false,false,false,false,false] NULL
[false,true,false,false,false,false,false,false,false,false,true,false] boolean
[false,false,true,true,true,false,false,false,false,false,true,false] integer
[false,false,false,false,false,true,true,false,false,false,true,false] double
[false,false,false,false,false,false,false,true,false,false,true,false] string
[false,false,false,false,false,false,false,true,false,false,true,false] string
[false,false,false,false,false,false,false,false,true,false,false,false] array
[false,false,false,false,false,false,false,false,false,true,false,false] object
[false,false,false,false,false,false,false,false,false,false,false,true] resource
4 3 1 App\Box bool(true)
bool(false)
TypeError: strlen(): Argument #1 ($string) must be of type string, array given
TypeError: count(): Argument #1 ($value) must be of type Countable|array, int given
TypeError: sizeof(): Argument #1 ($value) must be of type Countable|array, string given
TypeError: get_class(): Argument #1 ($object) must be of type object, int given
TypeError: strlen(): Argument #1 ($string) must be of type string, App\Box given
5 3 1

--TEST--
(string) of an integer and of a string, and of every other kind of operand: floats, booleans, null, arrays, objects with and without __toString, references, unset variables
--FILE--
<?php
set_error_handler(function ($no, $msg) { echo "  [$msg]\n"; return true; });
class Name { function __toString(): string { return 'name'; } }
class Bad {}
function conv(mixed $v): string { return (string) $v; }
function show(string $label, callable $f): void {
    try { $r = $f(); echo $label, ': ', var_export($r, true), "\n"; }
    catch (Throwable $e) { echo $label, ': ', get_class($e), ': ', $e->getMessage(), "\n"; }
}
foreach ([0, 5, -5, 9, 10, 99, 100, 12345, -12345, 1234567890123, PHP_INT_MAX, PHP_INT_MIN] as $n) {
    $s = (string) $n; $t = conv($n);
    echo $s, ' ', $t, ' ', strlen($s), ' ', $s === (string) $s ? 'same' : 'differs', "\n";
}
$i = 7; $s = 'abc'; $copy = $s; $same = (string) $s; $same .= 'd';
echo $s, ' ', $copy, ' ', $same, "\n";
foreach ([1.5, 1.0, -0.0, 1e100, NAN, INF, true, false, null, [], [1], new Name] as $v) {
    show('cast of ' . get_debug_type($v), fn () => (string) $v);
    show('function of ' . get_debug_type($v), fn () => conv($v));
}
show('object without __toString', fn () => (string) new Bad);
show('a reference to an int', function () { $a = 5; $r = &$a; return (string) $r; });
show('an unset variable', function () { return (string) $undefined; });
show('a local that is an int then a string', function () { $v = 5; $a = (string) $v; $v = 'x'; $b = (string) $v; return [$a, $b]; });
show('the same register written in a loop', function () { $out = []; for ($i = 0; $i < 3; $i++) { $t = (string) $i; $out[] = $t . '!'; } return $out; });
show('a destructor of the old value', function () { $o = new class { function __destruct() { echo "destruct\n"; } }; $o = (string) 5; echo "after\n"; return $o; });

--EXPECT--
0 0 1 same
5 5 1 same
-5 -5 2 same
9 9 1 same
10 10 2 same
99 99 2 same
100 100 3 same
12345 12345 5 same
-12345 -12345 6 same
1234567890123 1234567890123 13 same
9223372036854775807 9223372036854775807 19 same
-9223372036854775808 -9223372036854775808 20 same
abc abc abcd
cast of float: '1.5'
function of float: '1.5'
cast of float: '1'
function of float: '1'
cast of float: '-0'
function of float: '-0'
cast of float: '1.0E+100'
function of float: '1.0E+100'
  [unexpected NAN value was coerced to string]
cast of float: 'NAN'
  [unexpected NAN value was coerced to string]
function of float: 'NAN'
cast of float: 'INF'
function of float: 'INF'
cast of bool: '1'
function of bool: '1'
cast of bool: ''
function of bool: ''
cast of null: ''
function of null: ''
  [Array to string conversion]
cast of array: 'Array'
  [Array to string conversion]
function of array: 'Array'
  [Array to string conversion]
cast of array: 'Array'
  [Array to string conversion]
function of array: 'Array'
cast of Name: 'name'
function of Name: 'name'
object without __toString: Error: Object of class Bad could not be converted to string
a reference to an int: '5'
  [Undefined variable $undefined]
an unset variable: ''
a local that is an int then a string: array (
  0 => '5',
  1 => 'x',
)
the same register written in a loop: array (
  0 => '0!',
  1 => '1!',
  2 => '2!',
)
destruct
after
a destructor of the old value: '5'

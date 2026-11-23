--TEST--
A by-reference parameter and the calls PHP compiles away: an error for those, a notice for every real call
--FILE--
<?php
function byref(&$r) { return 1; }
function h() { return func_get_args(); }
class C { function m() { return static::class; } }
$a = [1, 2]; $s = 'abc'; $o = new C; $f = 'strlen';
$inc = tempnam(sys_get_temp_dir(), 'inc'); file_put_contents($inc, '<?php return [1];');
$tests = [
 'strlen var' => fn () => byref(strlen($s)), 'strlen const' => fn () => byref(strlen('d')), 'strlen 2 args?' => fn () => byref(\strlen($s)),
 'is_null' => fn () => byref(is_null($s)), 'is_bool' => fn () => byref(is_bool($s)), 'is_int' => fn () => byref(is_int($s)), 'is_long' => fn () => byref(is_long($s)), 'is_integer' => fn () => byref(is_integer($s)),
 'is_float' => fn () => byref(is_float($s)), 'is_double' => fn () => byref(is_double($s)), 'is_string' => fn () => byref(is_string($s)), 'is_array' => fn () => byref(is_array($s)),
 'is_object' => fn () => byref(is_object($s)), 'is_resource' => fn () => byref(is_resource($s)), 'is_scalar' => fn () => byref(is_scalar($s)), 'is_numeric' => fn () => byref(is_numeric($s)), 'is_callable' => fn () => byref(is_callable($s)), 'is_iterable' => fn () => byref(is_iterable($s)), 'is_countable' => fn () => byref(is_countable($s)),
 'boolval' => fn () => byref(boolval($s)), 'intval' => fn () => byref(intval($s)), 'intval base' => fn () => byref(intval($s, 8)), 'floatval' => fn () => byref(floatval($s)), 'doubleval' => fn () => byref(doubleval($s)), 'strval' => fn () => byref(strval($s)),
 'defined' => fn () => byref(defined('X')), 'defined var' => fn () => byref(defined($s)), 'constant' => fn () => byref(constant('PHP_EOL')),
 'chr const' => fn () => byref(chr(65)), 'chr var' => fn () => byref(chr($a[0])), 'ord const' => fn () => byref(ord('a')), 'ord var' => fn () => byref(ord($s)),
 'count' => fn () => byref(count($a)), 'count mode' => fn () => byref(count($a, 1)), 'sizeof' => fn () => byref(sizeof($a)),
 'get_class' => fn () => byref(get_class($o)), 'get_called_class' => fn () => byref(get_called_class()), 'gettype' => fn () => byref(gettype($s)), 'get_debug_type' => fn () => byref(get_debug_type($s)),
 'func_num_args' => fn () => byref(func_num_args()), 'func_get_args' => fn () => byref(func_get_args()), 'array_slice fga' => fn () => byref(array_slice(func_get_args(), 0)), 'array_slice' => fn () => byref(array_slice($a, 0)),
 'array_key_exists' => fn () => byref(array_key_exists(0, $a)), 'key_exists' => fn () => byref(key_exists(0, $a)), 'in_array const' => fn () => byref(in_array($s, ['a', 'b'])), 'in_array var' => fn () => byref(in_array($s, $a)),
 'call_user_func' => fn () => byref(call_user_func('h')), 'call_user_func_array' => fn () => byref(call_user_func_array('h', [])),
 'sprintf simple' => fn () => byref(sprintf('%s-%d', $s, 1)), 'sprintf pad' => fn () => byref(sprintf('%05d', 1)), 'sprintf no args' => fn () => byref(sprintf('x')),
 'ignore_user_abort' => fn () => byref(ignore_user_abort()), 'dirname' => fn () => byref(dirname(__FILE__)), 'dirname var' => fn () => byref(dirname($s)),
 'abs' => fn () => byref(abs(1)), 'implode' => fn () => byref(implode(',', $a)), 'array_keys' => fn () => byref(array_keys($a)), 'trim' => fn () => byref(trim($s)), 'min' => fn () => byref(min(1, 2)), 'json_encode' => fn () => byref(json_encode($a)),
 'compact' => fn () => byref(compact('s')), 'extract' => fn () => byref(extract($a)), 'get_object_vars' => fn () => byref(get_object_vars($o)), 'func via var' => fn () => byref($f($s)), 'fq strlen via ns' => fn () => byref(\is_null($s)),
 'named arg strlen' => fn () => byref(strlen(string: $s)), 'spread strlen' => fn () => byref(strlen(...[$s])), 'first class' => fn () => byref(strlen(...)),
 'static::class' => fn () => byref($o->m()), 'isset' => fn () => byref(isset($s)), 'empty' => fn () => byref(empty($s)), 'print' => fn () => byref(print ''), 'include' => fn () => byref(include $inc),
 'property_exists' => fn () => byref(property_exists($o, 'x')), 'class_exists 1' => fn () => byref(class_exists('C')), 'class_exists 2' => fn () => byref(class_exists('C', false)),
 'preg_match 2' => fn () => byref(preg_match('/a/', $s)), 'preg_replace 3' => fn () => byref(preg_replace('/a/', 'b', $s)), 'preg_replace 4' => fn () => byref(preg_replace('/a/', 'b', $s, 1)),
 'max 2' => fn () => byref(max(1, $a[0])), 'max 3' => fn () => byref(max(1, 2, 3)), 'max array' => fn () => byref(max($a)), 'trim 2' => fn () => byref(trim($s, 'a')), 'ltrim' => fn () => byref(ltrim($s)),
 'implode 1' => fn () => byref(implode($a)), 'dirname 2' => fn () => byref(dirname($s, 2)), 'strstr' => fn () => byref(strstr($s, 'b')), 'strstr 3' => fn () => byref(strstr($s, 'b', true)),
 'strpos' => fn () => byref(strpos($s, 'b')), 'strpos 3' => fn () => byref(strpos($s, 'b', 1)), 'str_contains' => fn () => byref(str_contains($s, 'b')), 'str_starts_with' => fn () => byref(str_starts_with($s, 'a')), 'str_ends_with' => fn () => byref(str_ends_with($s, 'c')),
 'substr 2' => fn () => byref(substr($s, 1)), 'substr 3' => fn () => byref(substr($s, 1, 1)), 'strtr 2' => fn () => byref(strtr($s, ['a' => 'b'])), 'strtr 3' => fn () => byref(strtr($s, 'a', 'b')),
 'str_replace 3' => fn () => byref(str_replace('a', 'b', $s)), 'str_replace 4' => fn () => byref(str_replace('a', 'b', $s, $cnt)), 'dechex' => fn () => byref(dechex(255)), 'explode' => fn () => byref(explode(',', $s)),
 'eval' => fn () => byref(eval('return [1];')), 'require' => fn () => byref(require $inc),
 'chr big' => fn () => byref(chr(300)), 'chr negative' => fn () => byref(chr(-1)), 'ord empty' => fn () => byref(ord('')), 'defined ns' => fn () => byref(defined('A\\B')), 'sprintf mismatch' => fn () => byref(sprintf('%s %s', $s)),
 'sprintf percent' => fn () => byref(sprintf('100%%')), 'array_slice fga 3' => fn () => byref(array_slice(func_get_args(), 0, 1)), 'array_slice fga var' => fn () => byref(array_slice(func_get_args(), $a[0])),
 'uppercase' => fn () => byref(STRLEN($s)), 'fq count' => fn () => byref(\count($a)), 'nested' => fn () => byref(array_values(array_filter($a))),
 'array_push ret' => fn () => byref(array_push($a, 3)), 'define' => fn () => byref(define('ZZ' . count($a), 1)),
];
foreach ($tests as $name => $test) {
    set_error_handler(function ($n, $m) use ($name) { echo "$name: notice\n"; return true; });
    try { $test(); } catch (Throwable $e) { echo $name, ': ', get_class($e), "\n"; }
    restore_error_handler();
}
unlink($inc);
--EXPECT--
strlen var: Error
strlen const: Error
strlen 2 args?: Error
is_null: Error
is_bool: Error
is_int: Error
is_long: Error
is_integer: Error
is_float: Error
is_double: Error
is_string: Error
is_array: Error
is_object: Error
is_resource: Error
is_scalar: Error
is_numeric: Error
is_callable: notice
is_iterable: notice
is_countable: notice
boolval: Error
intval: Error
intval base: notice
floatval: Error
doubleval: Error
strval: Error
defined: Error
defined var: notice
constant: notice
chr const: Error
chr var: notice
ord const: Error
ord var: notice
ord var: notice
count: Error
count mode: notice
sizeof: Error
get_class: Error
get_called_class: Error
gettype: Error
get_debug_type: notice
func_num_args: Error
func_get_args: Error
array_slice fga: Error
array_slice: notice
array_key_exists: Error
key_exists: notice
in_array const: Error
in_array var: Error
call_user_func: notice
call_user_func_array: notice
sprintf simple: Error
sprintf pad: notice
sprintf no args: Error
ignore_user_abort: notice
dirname: Error
dirname var: Error
abs: notice
implode: Error
array_keys: notice
trim: Error
min: Error
json_encode: notice
compact: notice
compact: notice
extract: notice
get_object_vars: notice
func via var: notice
fq strlen via ns: Error
named arg strlen: notice
spread strlen: notice
first class: Error
static::class: notice
isset: Error
empty: Error
print: Error
include: notice
property_exists: Error
class_exists 1: Error
class_exists 2: Error
preg_match 2: Error
preg_replace 3: Error
preg_replace 4: notice
max 2: Error
max 3: notice
max array: notice
trim 2: Error
ltrim: notice
implode 1: Error
dirname 2: Error
strstr: Error
strstr 3: Error
strpos: Error
strpos 3: Error
str_contains: Error
str_starts_with: Error
str_ends_with: notice
substr 2: Error
substr 3: Error
strtr 2: Error
strtr 3: Error
str_replace 3: Error
str_replace 4: notice
dechex: Error
explode: notice
eval: notice
require: notice
chr big: notice
chr big: notice
chr negative: notice
chr negative: notice
ord empty: notice
ord empty: notice
defined ns: notice
sprintf mismatch: ArgumentCountError
sprintf percent: Error
array_slice fga 3: notice
array_slice fga var: notice
uppercase: Error
fq count: Error
nested: notice
array_push ret: notice
define: notice

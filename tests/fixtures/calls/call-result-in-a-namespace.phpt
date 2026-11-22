--TEST--
A by-reference parameter and unqualified calls in a namespace: a notice unless the name is certain to be the global function
--FILE--
<?php
namespace App;
use function strlen as length;
use function count;
function byref(&$r) { return 1; }
function trim($x) { return 'own'; }
$a = [1]; $s = 'abc';
$tests = [
 'unqualified strlen' => fn () => byref(strlen($s)), 'fully qualified strlen' => fn () => byref(\strlen($s)), 'imported alias' => fn () => byref(length($s)), 'imported count' => fn () => byref(count($a)),
 'own trim' => fn () => byref(trim($s)), 'global trim' => fn () => byref(\trim($s)), 'unqualified in_array' => fn () => byref(in_array(1, $a)), 'namespace-relative' => fn () => byref(namespace\trim($s)),
 'unqualified is_array' => fn () => byref(is_array($a)), 'unqualified explode' => fn () => byref(explode(',', $s)),
];
foreach ($tests as $name => $test) {
    set_error_handler(function ($n, $m) use ($name) { echo "$name: notice\n"; return true; });
    try { $test(); } catch (\Throwable $e) { echo $name, ': ', get_class($e), "\n"; }
    restore_error_handler();
}
--EXPECT--
unqualified strlen: notice
fully qualified strlen: Error
imported alias: Error
imported count: Error
own trim: notice
global trim: Error
unqualified in_array: notice
namespace-relative: notice
unqualified is_array: notice
unqualified explode: notice

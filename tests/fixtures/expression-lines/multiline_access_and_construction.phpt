--TEST--
Multiline nullsafe, static, offset, constructor and first-class call lines
--FILE--
<?php
class P {
    public static $a = [];
    public function __construct($x = null) { if ($x === 'trace') mark(); }
    public function go($x = null) { mark(); }
}
function mark() {
    $trace = (new Exception)->getTrace();
    echo implode(' ', array_map(fn($x) => $x['function'] . '@' . ($x['line'] ?? '?'), $trace)), "\n";
}
function try_it($f) {
    try { $f(); } catch (Throwable $e) { echo $e->getMessage(), '@', $e->getLine(), "\n"; }
}
set_error_handler(function ($number, $message, $file, $line) { echo $message, '@', $line, "\n"; });
try_it(fn() => (new P)
    ?->
    nope
    (
    ));
try_it(fn() => (new P)
    ?->
    undefined
    ->go());
try_it(fn() => P
    ::
    $missing);
try_it(fn() => P
    ::$a
    [
       'missing'
    ]);
$array = [];
try_it(fn() => $array
    [
       'missing'
    ]);
try_it(fn() => new
    Missing
    (
        123
    ));
try_it(fn() => new
    P
    (
        'trace'
    ));
try_it(fn() => new
    Missing);
try_it(fn() => (new P)
    ->nope
    (
        ...
    ));
$f = 'missing';
try_it(fn() => $f
    (
        ...
    ));
$make = 'P';
try_it(fn() => new
    $make
    (
       'trace'
    ));
try_it(fn() => $f
    (
        0
    ));
try_it(fn() => P
    ::
    $a
    [
        (function() { mark(); return 'x'; })()
    ]);

--EXPECTF--
Call to undefined method P::nope()@17
Undefined property: P::$undefined@22
Call to a member function go() on null@23
Access to undeclared static property P::$missing@26
Undefined array key "missing"@30
Undefined array key "missing"@35
Class "Missing" not found@38
mark@4 __construct@43 {closure:%s:42}@12 try_it@42
Class "Missing" not found@48
Call to undefined method P::nope()@50
Call to undefined function missing()@55
mark@4 __construct@61 {closure:%s:60}@12 try_it@60
Call to undefined function missing()@65
mark@73 {closure:{closure:%s:69}:73}@73 {closure:%s:69}@12 try_it@69
Undefined array key "x"@73

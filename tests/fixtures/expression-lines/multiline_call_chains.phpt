--TEST--
Multiline calls and property reads report the relevant expression line
--FILE--
<?php
class P {
    function send($x) { return $this; }
    function typed(int $x) { return $this; }
    function then($f) { return $f(); }
    static function make($f = null) { return $f ? $f() : new P; }
}
function lines() { return implode(' ', array_map(fn($f) => ($f['function'] ?? '') . ':' . ($f['line'] ?? '?'), (new Exception)->getTrace())); }
function show($f) { try { $f(); } catch (Throwable $e) { echo get_class($e), ' line ', $e->getLine(), ': ', preg_replace('~/\S+/~', '', $e->getMessage()), "\n"; } }
show(fn() => (new P)
    ->send(1)
    ->nope(
        2
    ));
show(fn() => (new P)
    ->send(1)
    ->typed(
        'x'
    ));
show(fn() => (new P)
    ->send(1)
    ->then(
    ));
show(fn() => P
    ::nope(
        1
    ));
echo P
    ::make(
        'lines'
    ), "\n";
echo P::make()
    ::make(fn() =>
        lines()
    ), "\n";
echo lines(
    1,
    2
), "\n";
$f = 'lines';
echo $f
(
    1
), "\n";
$o = new
    P(
    );
echo (new P)
    ->send(1)
    ->then(...)
    ('lines'), "\n";
show(fn() => (new P)
    ->send(1)
    ->undefinedProp
    ->x());

--EXPECTF--
Error line 12: Call to undefined method P::nope()
TypeError line 4: P::typed(): Argument #1 ($x) must be of type int, string given, called in %s on line 17
ArgumentCountError line 5: Too few arguments to function P::then(), 0 passed in %s on line 22 and exactly 1 expected
Error line 25: Call to undefined method P::nope()
lines:6 make:29
lines:34 {closure:%s:33}:6 make:33
lines:36
lines:42
lines:5 then:51

Warning: Undefined property: P::$undefinedProp in %s on line 54
Error line 55: Call to a member function x() on null

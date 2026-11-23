--TEST--
A call result passed to a by-reference parameter: a notice for a call or `new`, an error for any other computed value, a returned reference bound or read
--FILE--
<?php
set_error_handler(function ($severity, $message, $file, $line) {
    echo "  [", $severity === E_NOTICE ? 'notice' : $severity, "] $message (line $line)\n";
    return true;
});
function byref(&$r) { $r = 'set'; return 1; }
function parts() { echo "  parts runs\n"; return ['a', 'b', 'c']; }
function &shared() { static $v = [1]; return $v; }
function grow($a) { $a[] = 1; return count($a); }
class K {
    function m() { return [1, 2]; }
    static function s() { return [1]; }
    function __invoke() { return [9]; }
    function takes(&$r) { $r[] = 0; return count($r); }
    static function also(&$r) { return 3; }
}
function show(string $label, callable $test) {
    echo $label, ":\n";
    try { $value = $test(); echo '  = ', var_export($value, true), "\n"; }
    catch (Throwable $e) { echo '  ', get_class($e), ': ', $e->getMessage(), "\n"; }
}

// Written as a call or as `new`: a notice, and the call goes on.
show('function result', fn () => byref(parts()));
show('method result', fn () => byref((new K)->m()));
show('nullsafe method result', fn () => byref((new K)?->m()));
show('static method result', fn () => byref(K::s()));
show('closure result', fn () => byref((fn () => [1])()));
show('invoked object', fn () => byref((new K)()));
show('new', fn () => byref(new stdClass));
show('named argument', fn () => byref(r: parts()));
show('to a method', fn () => (new K)->takes(parts()));
show('to a static method', fn () => K::also(parts()));
show('to a constructor', fn () => get_class(new class(parts()) { function __construct(&$r) {} }) !== '');
show('end', fn () => end(explode(',', 'a,b,c')));
show('array_pop', fn () => array_pop(parts()));
show('reset of a filter', fn () => reset(array_filter([0, 5, 6])));
show('sort', fn () => sort(parts()));
show('second argument', function () { $x = 1; return [byref($x), preg_match('/b/', 'abc', parts())]; });

// Any other computed value: an error.
show('ternary', fn () => byref(true ? parts() : parts()));
show('coalesce', fn () => byref(null ?? parts()));
show('cast', fn () => byref((array) parts()));
show('concatenation', fn () => byref('a' . 'b'));
show('literal', fn () => byref(5));
show('assignment', fn () => byref($x = parts()));
show('match', fn () => byref(match (1) { 1 => parts() }));
show('constant', fn () => byref(PHP_EOL));
show('clone', fn () => byref(clone new K));

// A function that returns a reference: bound without a notice.
show('returned reference', fn () => [(new K)->takes(shared()), count(shared())]);
// By value, the returned reference is read, not bound.
show('returned reference by value', fn () => [grow(shared()), count(shared())]);
$copy = shared(); $copy[] = 2; echo count(shared()), "\n";

// A handler that throws turns the notice into an exception before the call.
set_error_handler(function ($severity, $message) { throw new ErrorException($message, 0, $severity); });
show('notice as exception', fn () => byref(parts()));
restore_error_handler();
restore_error_handler();
echo "done\n";
--EXPECT--
function result:
  parts runs
  [notice] Only variables should be passed by reference (line 24)
  = 1
method result:
  [notice] Only variables should be passed by reference (line 25)
  = 1
nullsafe method result:
  [notice] Only variables should be passed by reference (line 26)
  = 1
static method result:
  [notice] Only variables should be passed by reference (line 27)
  = 1
closure result:
  [notice] Only variables should be passed by reference (line 28)
  = 1
invoked object:
  [notice] Only variables should be passed by reference (line 29)
  = 1
new:
  [notice] Only variables should be passed by reference (line 30)
  = 1
named argument:
  parts runs
  [notice] Only variables should be passed by reference (line 31)
  = 1
to a method:
  parts runs
  [notice] Only variables should be passed by reference (line 32)
  = 4
to a static method:
  parts runs
  [notice] Only variables should be passed by reference (line 33)
  = 3
to a constructor:
  parts runs
  [notice] Only variables should be passed by reference (line 34)
  = true
end:
  [notice] Only variables should be passed by reference (line 35)
  = 'c'
array_pop:
  parts runs
  [notice] Only variables should be passed by reference (line 36)
  = 'c'
reset of a filter:
  [notice] Only variables should be passed by reference (line 37)
  = 5
sort:
  parts runs
  [notice] Only variables should be passed by reference (line 38)
  = true
second argument:
  parts runs
  [notice] Only variables should be passed by reference (line 39)
  = array (
  0 => 1,
  1 => 1,
)
ternary:
  parts runs
  Error: byref(): Argument #1 ($r) could not be passed by reference
coalesce:
  parts runs
  Error: byref(): Argument #1 ($r) could not be passed by reference
cast:
  parts runs
  Error: byref(): Argument #1 ($r) could not be passed by reference
concatenation:
  Error: byref(): Argument #1 ($r) could not be passed by reference
literal:
  Error: byref(): Argument #1 ($r) could not be passed by reference
assignment:
  parts runs
  Error: byref(): Argument #1 ($r) could not be passed by reference
match:
  parts runs
  Error: byref(): Argument #1 ($r) could not be passed by reference
constant:
  Error: byref(): Argument #1 ($r) could not be passed by reference
clone:
  Error: byref(): Argument #1 ($r) could not be passed by reference
returned reference:
  = array (
  0 => 2,
  1 => 2,
)
returned reference by value:
  = array (
  0 => 3,
  1 => 2,
)
2
notice as exception:
  parts runs
  ErrorException: Only variables should be passed by reference
done

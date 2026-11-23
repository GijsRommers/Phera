--TEST--
A method site guards the receiver and calling scope before sending one local
--FILE--
<?php
class PlainTarget {
    function apply($v) { return ++$v; }
    private function secret($v) { return "private:$v"; }
}
class AliasTarget {
    function apply(&$v) { return $v += 10; }
}
function invoke($target, &$source) {
    $v = $source;
    echo $target->apply($v), '/', $v, "\n";
}
function viaAlias($target, &$source) {
    $v =& $source;
    echo $target->apply($v), '/', $v, "\n";
}
function secondSite($target, $v) { return $target->secret($v); }
$x = 1;
$a = new PlainTarget;
$b = new AliasTarget;
foreach ([$a, $a, $b, $b, $a] as $receiver) invoke($receiver, $x);
viaAlias($a, $x);
viaAlias($b, $x);
$closure = function ($receiver, $v) { return $receiver->secret($v); };
$bound = Closure::bind($closure, null, PlainTarget::class);
echo $bound($a, $x), "\n";
try { $closure($a, $x); } catch (Error $e) { echo "scope\n"; }
try { secondSite($a, $x); } catch (Error $e) { echo "function scope\n"; }
echo $bound($a, $x), "\n";
--EXPECT--
2/1
2/1
11/11
11/11
2/1
2/1
11/11
private:11
scope
function scope
private:11

--TEST--
Nested fibers restore current fiber identity
--FILE--
<?php
$outer = new Fiber(function () {
    $parent = Fiber::getCurrent();
    $inner = new Fiber(function () use ($parent) {
        var_dump($parent->isRunning(), Fiber::getCurrent() !== $parent);
        return Fiber::suspend('inner');
    });
    var_dump($inner->start(), Fiber::getCurrent() === $parent);
    var_dump(Fiber::suspend('outer'), $inner->resume('child result'), $inner->getReturn());
});
var_dump($outer->start(), Fiber::getCurrent(), $outer->resume('parent result'));
--EXPECT--
bool(true)
bool(true)
string(5) "inner"
bool(true)
string(13) "parent result"
NULL
string(12) "child result"
string(5) "outer"
NULL
NULL

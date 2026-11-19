--TEST--
An exception trace crosses a resumed fiber at its call site
--FILE--
<?php
$fiber = new Fiber(function () {
    Fiber::suspend();
    return (new Exception())->getTrace();
});
$fiber->start();
$fiber->resume();
$trace = $fiber->getReturn();
var_dump(count($trace), $trace[1]['class'], $trace[1]['type'], $trace[1]['function']);
--EXPECT--
int(2)
string(5) "Fiber"
string(2) "->"
string(6) "resume"

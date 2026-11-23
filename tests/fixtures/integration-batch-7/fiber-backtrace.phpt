--TEST--
Live and reflected fiber traces respect their own argument options
--FILE--
<?php
ini_set('zend.exception_ignore_args', '1');
function probe($value) {
    foreach (debug_backtrace() as $frame) {
        printf("%s %d %d\n", $frame['function'], isset($frame['object']), isset($frame['args']));
    }
    foreach ((new Exception)->getTrace() as $frame) {
        printf("exception %s %d\n", $frame['function'], isset($frame['args']));
    }
    Fiber::suspend($value);
}
function enter(Fiber $fiber) {
    $fiber->start('value');
}
$fiber = new Fiber('probe');
enter($fiber);
foreach ((new ReflectionFiber($fiber))->getTrace() as $frame) {
    printf("reflection %s %d\n", $frame['function'], isset($frame['args']));
}
$fiber->resume();
--EXPECT--
probe 0 1
start 1 1
enter 0 1
exception probe 0
exception start 0
exception enter 0
reflection suspend 1
reflection probe 1

--TEST--
Fibers share PHP globals across suspension and asynchronous I/O
--FILE--
<?php
$counter = 2;
$fiber = new Fiber(function () {
    global $counter;
    $counter += 2;
    usleep(1);
    Fiber::suspend($counter);
    $GLOBALS['counter'] += 2;
    return $counter;
});
var_dump($fiber->start(), $counter);
$counter += 1;
var_dump($fiber->resume(), $fiber->getReturn(), $counter);
--EXPECT--
int(4)
int(4)
NULL
int(7)
int(7)

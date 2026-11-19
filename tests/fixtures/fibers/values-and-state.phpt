--TEST--
Fiber transfers values and retains the callback result
--FILE--
<?php
var_dump(Fiber::getCurrent());
$fiber = new Fiber(function ($left, $right) {
    $self = Fiber::getCurrent();
    var_dump($self->isStarted(), $self->isRunning(), $self->isSuspended());
    return [Fiber::suspend($left), $right];
});
var_dump($fiber->isStarted(), $fiber->start(right: 'right', left: 'left'));
var_dump($fiber->isSuspended(), Fiber::getCurrent());
var_dump($fiber->resume('response'), $fiber->isTerminated(), $fiber->getReturn());
--EXPECT--
NULL
bool(true)
bool(true)
bool(false)
bool(false)
string(4) "left"
bool(true)
NULL
NULL
bool(true)
array(2) {
  [0]=>
  string(8) "response"
  [1]=>
  string(5) "right"
}

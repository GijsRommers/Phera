--TEST--
Invalid fiber operations produce PHP FiberError messages
--FILE--
<?php
function attempt($call) {
    try { $call(); } catch (Error $caught) {
        echo get_class($caught), ': ', $caught->getMessage(), "\n";
    }
}
attempt(fn () => Fiber::suspend());
attempt(fn () => new FiberError());
$fiber = new Fiber(function () { attempt(fn () => Fiber::getCurrent()->resume()); });
attempt(fn () => $fiber->getReturn());
attempt(fn () => $fiber->resume());
$fiber->start();
attempt(fn () => $fiber->start());
attempt(fn () => $fiber->throw(new Exception()));
--EXPECT--
FiberError: Cannot suspend outside of a fiber
Error: The "FiberError" class is reserved for internal use and cannot be manually instantiated
FiberError: Cannot get fiber return value: The fiber has not been started
FiberError: Cannot resume a fiber that is not suspended
FiberError: Cannot resume a fiber that is not suspended
FiberError: Cannot start a fiber that has already been started
FiberError: Cannot resume a fiber that is not suspended

--TEST--
ReflectionFiber inspects suspension without resuming the callable
--FILE--
<?php
$callback = function () { Fiber::suspend('paused'); return 'saved'; };
$fiber = new Fiber($callback);
$reflection = new ReflectionFiber($fiber);
var_dump($reflection->getFiber() === $fiber, $reflection->getCallable() === $callback);
try { $reflection->getExecutingFile(); } catch (Error $caught) { echo $caught->getMessage(), "\n"; }
$fiber->start();
var_dump($reflection->getExecutingFile() === __FILE__, $reflection->getExecutingLine());
$trace = $reflection->getTrace(DEBUG_BACKTRACE_IGNORE_ARGS);
var_dump($trace[0]['class'], $trace[0]['function'], array_key_exists('args', $trace[0]));
$fiber->resume();
try { $reflection->getCallable(); } catch (Error $caught) { echo $caught->getMessage(), "\n"; }
--EXPECT--
bool(true)
bool(true)
Cannot fetch information from a fiber that has not been started or is terminated
bool(true)
int(2)
string(5) "Fiber"
string(7) "suspend"
bool(false)
Cannot fetch the callable from a fiber that has terminated

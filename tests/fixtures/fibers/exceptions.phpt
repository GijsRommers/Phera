--TEST--
Fiber throw injects the same exception and resume propagates escapes
--FILE--
<?php
$exception = new RuntimeException('injected');
$fiber = new Fiber(function () use ($exception) {
    try { Fiber::suspend('ready'); }
    catch (RuntimeException $received) {
        var_dump($received === $exception);
        Fiber::suspend('caught');
        throw new LogicException('escaped');
    } finally {
        echo "finally\n";
    }
});
var_dump($fiber->start(), $fiber->throw($exception));
try { $fiber->resume(); } catch (LogicException $caught) { echo $caught->getMessage(), "\n"; }
try { $fiber->getReturn(); } catch (FiberError $caught) { echo $caught->getMessage(), "\n"; }
--EXPECT--
bool(true)
string(5) "ready"
string(6) "caught"
finally
escaped
Cannot get fiber return value: The fiber threw an exception

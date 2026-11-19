--TEST--
Request shutdown closes suspended fibers and runs finally blocks
--FILE--
<?php
$fiber = new Fiber(function () {
    try { Fiber::suspend(); }
    catch (Throwable $caught) { echo "caught\n"; }
    finally {
        var_dump(Fiber::getCurrent() instanceof Fiber);
        try { Fiber::suspend(); }
        catch (FiberError $caught) { echo $caught->getMessage(), "\n"; }
    }
    echo "unreachable\n";
});
$fiber->start();
register_shutdown_function(fn () => print "shutdown callback\n");
echo "main\n";
--EXPECT--
main
shutdown callback
bool(true)
Cannot suspend in a force-closed fiber

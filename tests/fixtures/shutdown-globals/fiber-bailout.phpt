--TEST--
Shutdown can inspect the globally held fiber that exited fatally
--FILE--
<?php
register_shutdown_function(static function () {
    global $currentFiber;
    try {
        $currentFiber->getReturn();
    } catch (FiberError $error) {
        echo $error->getMessage(), "\n";
    }
});
$currentFiber = new Fiber(static function () {
    var_dump(new class {
        public function __debugInfo() { return 'invalid'; }
    });
});
$currentFiber->start();
--EXPECTF--
Fatal error: __debuginfo() must return an array in %s on line 11
Cannot get fiber return value: The fiber exited with a fatal error

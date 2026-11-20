--TEST--
A fatal request skips generator finally blocks at shutdown
--FILE--
<?php
function gen($name) { try { yield; } finally { echo "close $name\n"; } }
$old = gen('old');
$old->rewind();
register_shutdown_function(function () {
    echo "shutdown\n";
    $GLOBALS['new'] = gen('new');
    $GLOBALS['new']->rewind();
});
var_dump(new class { function __debugInfo() { return false; } });
--EXPECTF--
Fatal error: __debuginfo() must return an array in %s on line 10
shutdown

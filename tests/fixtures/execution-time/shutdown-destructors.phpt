--TEST--
Shutdown destructors
--INI--
fatal_error_backtraces=1
--FILE--
<?php
class RequestObject {
    public function __construct(public string $name) {}
    public function __destruct() { echo "destruct: $this->name\n"; }
}
$before = new RequestObject('before');
$label = 'shutdown';
register_shutdown_function(function () {
    global $label, $after;
    echo $label, "\n";
    $after = new RequestObject('after');
});
ob_start();
set_time_limit(1);
while (true) {}
--EXPECTF--
Fatal error: Maximum execution time of 1 second exceeded in %sshutdown-destructors.php on line 15
Stack trace:
#0 {main}
shutdown
destruct: after

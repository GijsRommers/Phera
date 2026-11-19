--TEST--
Include compile error
--INI--
fatal_error_backtraces=1
--FILE--
<?php
register_shutdown_function(function () {
    $last = error_get_last();
    echo $last['type'], ': ', $last['message'], "\n";
});
function load_bad_file($argument) { include __DIR__ . '/bad-declaration.inc'; }
load_bad_file('long string parameter');
--EXPECTF--
Fatal error: A void function must not return a value in %sbad-declaration.inc on line 1
Stack trace:
#0 %sinclude-compile-error.php(7): load_bad_file('long string par...')
#1 {main}
64: A void function must not return a value

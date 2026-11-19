--TEST--
A warning raised in a fiber becomes the request's last error
--FILE--
<?php
error_clear_last();
$fiber = new Fiber(function () {
    @trigger_error('fiber warning', E_USER_WARNING);
    var_dump(error_get_last()['message']);
});
$fiber->start();
$error = error_get_last();
var_dump($error['type'], $error['message'], $error['file'] === __FILE__, $error['line']);
--EXPECT--
string(13) "fiber warning"
int(512)
string(13) "fiber warning"
bool(true)
int(4)

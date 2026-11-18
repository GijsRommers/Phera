--TEST--
User declarations resolve before the omitted item policy
--FILE--
<?php
// This fixture asserts behavior specific to Phera’s deprecated-item policy.
namespace {
    function strftime() { return 'global'; }
    const MT_RAND_PHP = 'global';
}
namespace App {
    function lcg_value() { return 'local'; }
    const FILTER_SANITIZE_STRING = 'local';
    echo strftime(), ' ', MT_RAND_PHP, ' ', lcg_value(), ' ', FILTER_SANITIZE_STRING, "\n";
    var_dump(function_exists('strftime'), defined('MT_RAND_PHP'), is_callable('App\lcg_value'));
}
--EXPECT--
global global local local
bool(true)
bool(true)
bool(true)

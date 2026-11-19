--TEST--
Persistent PHP 8.5 folds retain runtime constants and deprecations
--FILE--
<?php
declare(strict_types=1);
define("23", true);
define("2.3", true);
var_dump(defined(23), defined(2.3));

var_dump(extension_loaded('cOrE'), extension_loaded('phera_unknown_test_module'));
var_dump(dirname('/a/b/'), dirname('/one'), dirname('leaf'));
var_dump(constant('PHP_INT_MAX') === PHP_INT_MAX);
if (true) {
    define('PHERA_FOLDING_TEST_RUNTIME', 'runtime');
}
var_dump(constant('PHERA_FOLDING_TEST_RUNTIME'));
set_error_handler(function ($level, $message) { echo "$message\n"; return true; });
var_dump(constant('E_STRICT'));
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(false)
string(2) "/a"
string(1) "/"
string(1) "."
bool(true)
string(7) "runtime"
Constant E_STRICT is deprecated since 8.4, the error level was removed
int(2048)

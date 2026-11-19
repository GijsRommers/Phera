--TEST--
PHP 8.5 names arrow functions after their source
--FILE--
<?php
$callback = fn() => debug_print_backtrace();
$callback();
--EXPECTF--
#0 %s/arrow.php(3): {closure:%s/arrow.php:2}()

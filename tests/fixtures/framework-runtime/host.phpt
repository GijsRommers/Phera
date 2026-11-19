--TEST--
Process host name and uname modes
--FILE--
<?php
var_dump(gethostname() === php_uname('n'), strlen(php_uname('s')) > 0);
var_dump(php_uname() === implode(' ', [php_uname('s'), php_uname('n'), php_uname('r'), php_uname('v'), php_uname('m')]));
foreach (['', 'z'] as $mode) {
    try { php_uname($mode); } catch (ValueError $error) { echo $error->getMessage(), "\n"; }
}
--EXPECT--
bool(true)
bool(true)
bool(true)
php_uname(): Argument #1 ($mode) must be a single character
php_uname(): Argument #1 ($mode) must be one of "a", "m", "n", "r", "s", or "v"

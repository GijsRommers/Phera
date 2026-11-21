--TEST--
Startup-disabled assertions skips a direct assertion and cannot be enabled at run time
--INI--
zend.assertions=-1
--FILE--
<?php
function assertion_argument(): bool {
    echo "evaluated\n";
    return false;
}

set_error_handler(function ($level, $message) { echo $message, "\n"; });
var_dump(ini_get('zend.assertions'));
var_dump(assert(assertion_argument()));
var_dump(ini_set('zend.assertions', '0'));
var_dump(ini_set('zend.assertions', '1'));
var_dump(ini_set('zend.assertions', '-1'));
$fn = 'assert';
var_dump($fn(assertion_argument()));
var_dump(call_user_func('assert', assertion_argument()));
?>
--EXPECT--
string(2) "-1"
bool(true)
zend.assertions may be completely enabled or disabled only in php.ini
bool(false)
zend.assertions may be completely enabled or disabled only in php.ini
bool(false)
string(2) "-1"
evaluated
bool(true)
evaluated
bool(true)

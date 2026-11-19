--TEST--
Changing the reporting mask in @ expressions and on unwind and yield
--FILE--
<?php
function report() {
    var_dump(error_reporting(), ini_get('error_reporting'));
}
function warning($message) {
    trigger_error($message, E_USER_WARNING);
}
function changeMask() {
    report();
    warning('suppressed');
    var_dump(error_reporting(E_ALL));
    warning('enabled by error_reporting');
    @warning('suppressed again');
    warning('restored by the inner @');
}
error_reporting(E_ALL & ~E_USER_WARNING);
@changeMask();
report();

function changeIni() {
    var_dump(ini_set('error_reporting', (string) E_ALL));
    warning('enabled by ini_set');
    error_reporting(0);
    throw new Exception('abort the expression');
}
try {
    @@changeIni();
} catch (Exception) {
    report();
}

function silenceAndThrow() {
    @call_user_func(function () {
        error_reporting(E_ALL & ~E_USER_WARNING);
        throw new Exception('cross a native call');
    });
}
try {
    @silenceAndThrow();
} catch (Exception) {
    report();
}

function generator() {
    @(yield error_reporting());
    report();
}
$generator = generator();
var_dump($generator->current());
report();
$generator->next();
report();

function oversizedMask() {
    error_reporting(1 << 40);
    report();
}
@oversizedMask();
report();
--EXPECTF--
int(4437)
string(5) "30207"
int(4437)

Warning: enabled by error_reporting in %s on line 6

Warning: restored by the inner @ in %s on line 6
int(30719)
string(5) "30719"
string(5) "30719"

Warning: enabled by ini_set in %s on line 6
int(30719)
string(1) "0"
int(30207)
string(5) "30207"
int(4437)
int(4437)
string(5) "30207"
int(30207)
string(5) "30207"
int(30207)
string(5) "30207"
int(0)
string(13) "1099511627776"
int(30207)
string(13) "1099511627776"

--TEST--
PHP finds the function before it evaluates the argument: an argument whose evaluation declares the namespace's function does not change the call it is the argument of, only later sites.
--FILE--
<?php
// PHP finds the function before it evaluates the argument: an argument whose
// evaluation declares the namespace's function does not change the call it
// is the argument of, only later sites.
namespace App;
final class Lazy {
    public function __get($name) {
        if (!\function_exists('App\is_null')) {
            function is_null($value) { return 'App\is_null'; }
        }
        return null;
    }
}
function check($box) { return is_null($box->anything); }
$box = new Lazy;
var_dump(check($box), check($box));
function fresh($value) { return is_null($value); }
var_dump(fresh(null));
--EXPECT--
bool(true)
bool(true)
string(11) "App\is_null"

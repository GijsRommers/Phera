--TEST--
A by-reference parameter that a callback fills with a value warns and works on a temporary
--FILE--
<?php
function t(string $label, callable $f): void { try { echo $label, ": "; var_dump($f()); } catch (Throwable $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; } }
t("call_user_func pop", fn() => call_user_func('array_pop', [1, 2]));
t("call_user_func_array push", fn() => call_user_func_array('array_push', [[1], 2]));
t("call_user_func_array sort", fn() => call_user_func_array('sort', [[3, 1, 2]]));
t("call_user_func usort", fn() => call_user_func('usort', [3, 1], fn($a, $b) => $a <=> $b));
t("array_map shift", fn() => array_map('array_shift', [[1, 2], [3, 4]]));
t("array_map end", fn() => array_map('end', [[1, 2], [3]]));
t("variable stays", function () { $a = [3, 1, 2]; $r = call_user_func('sort', $a); return [$r, $a]; });
t("PHP function", fn() => array_map(function (&$x) { $x++; return $x; }, [1, 2]));
t("wrong type", fn() => call_user_func('sort', 5));
t("null", fn() => call_user_func('array_pop', null));
t("first-class callable with a variable", function () { $p = array_pop(...); $a = [1, 2]; return [$p($a), $a]; });
t("a real reference is kept", function () { $a = [2, 1]; call_user_func_array('sort', [&$a]); return $a; });
echo "done\n";
--EXPECTF--
call_user_func pop: 
Warning: array_pop(): Argument #1 ($array) must be passed by reference, value given in %s on line 3
int(2)
call_user_func_array push: 
Warning: array_push(): Argument #1 ($array) must be passed by reference, value given in %s on line 4
int(2)
call_user_func_array sort: 
Warning: sort(): Argument #1 ($array) must be passed by reference, value given in %s on line 5
bool(true)
call_user_func usort: 
Warning: usort(): Argument #1 ($array) must be passed by reference, value given in %s on line 6
bool(true)
array_map shift: 
Warning: array_shift(): Argument #1 ($array) must be passed by reference, value given in %s on line 7

Warning: array_shift(): Argument #1 ($array) must be passed by reference, value given in %s on line 7
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(3)
}
array_map end: 
Warning: end(): Argument #1 ($array) must be passed by reference, value given in %s on line 8

Warning: end(): Argument #1 ($array) must be passed by reference, value given in %s on line 8
array(2) {
  [0]=>
  int(2)
  [1]=>
  int(3)
}
variable stays: 
Warning: sort(): Argument #1 ($array) must be passed by reference, value given in %s on line 9
array(2) {
  [0]=>
  bool(true)
  [1]=>
  array(3) {
    [0]=>
    int(3)
    [1]=>
    int(1)
    [2]=>
    int(2)
  }
}
PHP function: 
Warning: {closure:{closure:%s:10}:10}(): Argument #1 ($x) must be passed by reference, value given in %s on line 10

Warning: {closure:{closure:%s:10}:10}(): Argument #1 ($x) must be passed by reference, value given in %s on line 10
array(2) {
  [0]=>
  int(2)
  [1]=>
  int(3)
}
wrong type: 
Warning: sort(): Argument #1 ($array) must be passed by reference, value given in %s on line 11
TypeError: sort(): Argument #1 ($array) must be of type array, int given
null: 
Warning: array_pop(): Argument #1 ($array) must be passed by reference, value given in %s on line 12
TypeError: array_pop(): Argument #1 ($array) must be of type array, null given
first-class callable with a variable: array(2) {
  [0]=>
  int(2)
  [1]=>
  array(1) {
    [0]=>
    int(1)
  }
}
a real reference is kept: array(2) {
  [0]=>
  int(1)
  [1]=>
  int(2)
}
done

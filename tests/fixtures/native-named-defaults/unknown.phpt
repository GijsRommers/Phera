--TEST--
Native named defaults: unknown
--FILE--
<?php
foreach ([
    fn() => array_keys([], strict: true),
    fn() => call_user_func_array('array_keys', [[], 'strict' => true]),
    fn() => array_keys(...[[], 'strict' => true]),
] as $call) {
    try {
        $call();
    } catch (ArgumentCountError $e) {
        echo $e->getMessage(), "\n";
    }
}
var_dump(array_keys([null, 1], null, true));
?>
--EXPECT--
array_keys(): Argument #2 ($filter_value) must be passed explicitly, because the default value is not known
array_keys(): Argument #2 ($filter_value) must be passed explicitly, because the default value is not known
array_keys(): Argument #2 ($filter_value) must be passed explicitly, because the default value is not known
array(1) {
  [0]=>
  int(0)
}

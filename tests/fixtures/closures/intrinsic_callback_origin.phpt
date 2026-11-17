--TEST--
intrinsic callback origin
--FILE--
<?php
declare(strict_types=1);
var_dump(array_map('is_callable', [fn()=>1], [1]));
$f=fn(int $n)=>$n+1;
var_dump(array_map('call_user_func', [$f], ['3']));
var_dump(array_map('call_user_func_array', [$f], [['4']]));
try { call_user_func('is_callable', fn()=>1, 1); }
catch (TypeError $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
array(1) {
  [0]=>
  bool(true)
}
array(1) {
  [0]=>
  int(4)
}
array(1) {
  [0]=>
  int(5)
}
is_callable(): Argument #2 ($syntax_only) must be of type bool, int given

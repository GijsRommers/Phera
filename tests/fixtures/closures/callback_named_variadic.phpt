--TEST--
callback named variadic
--FILE--
<?php
$f=fn($a,...$rest)=>[$a,$rest];var_dump(call_user_func_array($f,["a"=>1,"b"=>2,"c"=>3]));
--EXPECT--
array(2) {
  [0]=>
  int(1)
  [1]=>
  array(2) {
    ["b"]=>
    int(2)
    ["c"]=>
    int(3)
  }
}

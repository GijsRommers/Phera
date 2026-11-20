--TEST--
An array literal reads a compiled variable key after evaluating its value
--FILE--
<?php
$key = 1;
var_dump([$key => ($key = 5)]);
$keys = [1];
var_dump([$keys[0] => ($keys[0] = 5)]);
set_error_handler(static function ($level, $message) {
    echo $message, "\n";
    global $value, $recoveredKey;
    $value = 2;
    $recoveredKey = 42;
    return true;
});
var_dump([$missing => print("value first\n")]);
$value = 1;
$alias =& $value;
var_dump([$missing => $alias]);
unset($recoveredKey);
var_dump([$recoveredKey => $missingValue]);
--EXPECT--
array(1) {
  [5]=>
  int(5)
}
array(1) {
  [1]=>
  int(5)
}
value first
Undefined variable $missing
array(1) {
  [""]=>
  int(1)
}
Undefined variable $missing
array(1) {
  [""]=>
  int(2)
}
Undefined variable $missingValue
array(1) {
  [42]=>
  NULL
}

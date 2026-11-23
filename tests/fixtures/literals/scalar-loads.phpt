--TEST--
Immediate scalar loads preserve literal values in plain and guarded calls
--FILE--
<?php
function implicitNull() {}
function literalValues(bool $flag, ?bool $default = null): array {
    var_dump($default);
    return [
        $flag ? true : false,
        $flag ? null : true,
        -2147483648, 2147483647, 2147483648, 0.5, 'text', [],
    ];
}

class Guard {
    function __destruct() { echo "released\n"; }
}

var_dump(implicitNull());
var_dump(literalValues(true));
$guard = new Guard();
var_dump(literalValues(false, false));
unset($guard);
--EXPECT--
NULL
NULL
array(8) {
  [0]=>
  bool(true)
  [1]=>
  NULL
  [2]=>
  int(-2147483648)
  [3]=>
  int(2147483647)
  [4]=>
  int(2147483648)
  [5]=>
  float(0.5)
  [6]=>
  string(4) "text"
  [7]=>
  array(0) {
  }
}
bool(false)
array(8) {
  [0]=>
  bool(false)
  [1]=>
  bool(true)
  [2]=>
  int(-2147483648)
  [3]=>
  int(2147483647)
  [4]=>
  int(2147483648)
  [5]=>
  float(0.5)
  [6]=>
  string(4) "text"
  [7]=>
  array(0) {
  }
}
released

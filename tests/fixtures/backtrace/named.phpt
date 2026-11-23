--TEST--
Named fixed arguments appear in parameter order
--FILE--
<?php
function inspect($a = 1, $b = 2) { var_export(debug_backtrace()[0]['args']); }
inspect(b: 7);
--EXPECT--
array (
  0 => 1,
  1 => 7,
)

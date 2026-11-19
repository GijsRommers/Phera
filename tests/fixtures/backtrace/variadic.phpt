--TEST--
Trace arguments flatten variadics and preserve extra names
--FILE--
<?php
function inspect($v, ...$rest) { var_export(debug_backtrace()[0]['args']); }
inspect(1, 2, name: 3);
--EXPECT--
array (
  0 => 1,
  1 => 2,
  'name' => 3,
)

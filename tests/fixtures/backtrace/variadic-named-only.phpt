--TEST--
Named variadic arguments do not add omitted fixed arguments
--FILE--
<?php
function inspect($first = 1, ...$rest) {
    var_export(debug_backtrace()[0]['args']);
}
inspect(label: 42);
--EXPECT--
array (
  'label' => 42,
)

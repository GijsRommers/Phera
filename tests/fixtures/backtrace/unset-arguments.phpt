--TEST--
Unset arguments are shown as null at their original index
--FILE--
<?php
function inspect($a, $b) {
    unset($a);
    $b = 42;
    var_export(debug_backtrace()[0]['args']);
}
inspect('a', 'b');
--EXPECT--
array (
  0 => NULL,
  1 => 42,
)

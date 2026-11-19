--TEST--
Argument builtins observe defaults, positional surplus and updated fixed arguments
--FILE--
<?php
function captureArgs($first, $second = 'default', ...$more) {
    $first = 'updated';
    echo func_num_args(), "\n";
    var_export(func_get_args());
    echo "\n";
    var_export(array_slice(func_get_args(), 1));
    echo "\n";
}
captureArgs('one');
captureArgs('one', 'two', 'three', extra: 'named');
--EXPECT--
1
array (
  0 => 'updated',
)
array (
)
3
array (
  0 => 'updated',
  1 => 'two',
  2 => 'three',
)
array (
  0 => 'two',
  1 => 'three',
)

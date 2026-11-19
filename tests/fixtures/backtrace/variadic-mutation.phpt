--TEST--
Variadic trace values are the original arguments
--FILE--
<?php
function inspect(...$rest) {
    $rest[0] = 'modified';
    $rest['name'] = 'modified';
    unset($rest);
    var_export(debug_backtrace()[0]['args']);
}
inspect('original', name: 'original');
--EXPECT--
array (
  0 => 'original',
  'name' => 'original',
)

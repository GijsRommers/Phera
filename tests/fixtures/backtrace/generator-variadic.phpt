--TEST--
Suspended generators retain their original variadic arguments
--FILE--
<?php
function inside() {
    $frame = debug_backtrace(0, 2)[1];
    var_export($frame['args']);
    yield 1;
}
function outside(...$rest) {
    $rest[0] = 'changed';
    unset($rest);
    yield from inside();
}
outside('original', label: 'named')->rewind();
--EXPECT--
array (
  0 => 'original',
  'label' => 'named',
)

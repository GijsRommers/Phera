--TEST--
SplFixedArray serialization bytes and restored state
--FILE--
<?php
$original = new SplFixedArray(3);
$original[1] = 'item';
$wire = serialize($original);
echo $wire, "\n";
$copy = unserialize($wire);
echo get_class($copy), '|', (int) (serialize($copy) === $wire), "\n";
var_export($copy->toArray());
echo "\n";
?>
--EXPECT--
O:13:"SplFixedArray":3:{i:0;N;i:1;s:4:"item";i:2;N;}
SplFixedArray|1
array (
  0 => NULL,
  1 => 'item',
  2 => NULL,
)

--TEST--
SplMinHeap serialization bytes and restored state
--FILE--
<?php
$original = new SplMinHeap;
foreach ([5, 1, 9, 3] as $value) {
    $original->insert($value);
}
$wire = serialize($original);
echo $wire, "\n";
$copy = unserialize($wire);
echo get_class($copy), '|', (int) (serialize($copy) === $wire), "\n";
while (!$copy->isEmpty()) {
    echo $copy->extract(), "\n";
}
?>
--EXPECT--
O:10:"SplMinHeap":2:{i:0;a:0:{}i:1;a:2:{s:5:"flags";i:0;s:13:"heap_elements";a:4:{i:0;i:1;i:1;i:3;i:2;i:9;i:3;i:5;}}}
SplMinHeap|1
1
3
5
9

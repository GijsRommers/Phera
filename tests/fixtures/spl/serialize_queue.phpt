--TEST--
SplQueue serialization bytes and restored state
--FILE--
<?php
$original = new SplQueue;
$original->push('one');
$original->push(7);
$wire = serialize($original);
echo $wire, "\n";
$copy = unserialize($wire);
echo get_class($copy), '|', (int) (serialize($copy) === $wire), "\n";
echo $copy->getIteratorMode(), '|', $copy->dequeue(), '|', $copy->dequeue(), "\n";
?>
--EXPECT--
O:8:"SplQueue":3:{i:0;i:4;i:1;a:2:{i:0;s:3:"one";i:1;i:7;}i:2;a:0:{}}
SplQueue|1
4|one|7

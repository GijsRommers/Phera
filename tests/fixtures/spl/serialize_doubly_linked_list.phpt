--TEST--
SplDoublyLinkedList serialization bytes and restored state
--FILE--
<?php
$original = new SplDoublyLinkedList;
$original->push('one');
$original->push(7);
$wire = serialize($original);
echo $wire, "\n";
$copy = unserialize($wire);
echo get_class($copy), '|', (int) (serialize($copy) === $wire), "\n";
echo $copy->getIteratorMode(), '|', $copy->shift(), '|', $copy->shift(), "\n";
?>
--EXPECT--
O:19:"SplDoublyLinkedList":3:{i:0;i:0;i:1;a:2:{i:0;s:3:"one";i:1;i:7;}i:2;a:0:{}}
SplDoublyLinkedList|1
0|one|7

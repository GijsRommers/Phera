--TEST--
ArrayObject serialization bytes and restored state
--FILE--
<?php
class RoundTripIterator extends ArrayIterator {}
$original = new ArrayObject(['red' => 3, 'blue' => 8], ArrayObject::ARRAY_AS_PROPS);
$original->setIteratorClass(RoundTripIterator::class);
$wire = serialize($original);
echo $wire, "\n";
$copy = unserialize($wire);
echo get_class($copy), '|', (int) (serialize($copy) === $wire), "\n";
echo $copy->red, '|', get_class($copy->getIterator()), '|', $copy->getFlags(), "\n";
?>
--EXPECT--
O:11:"ArrayObject":4:{i:0;i:2;i:1;a:2:{s:3:"red";i:3;s:4:"blue";i:8;}i:2;a:0:{}i:3;s:17:"RoundTripIterator";}
ArrayObject|1
3|RoundTripIterator|2

--TEST--
ArrayIterator serialization bytes and restored state
--FILE--
<?php
$original = new ArrayIterator([3 => 'x', 'id' => 7]);
$original->seek(1);
$wire = serialize($original);
echo $wire, "\n";
$copy = unserialize($wire);
echo get_class($copy), '|', (int) (serialize($copy) === $wire), "\n";
echo $copy->key(), '|', $copy->current(), '|', count($copy), "\n";
foreach ($copy as $key => $value) {
    echo $key, '=', $value, "\n";
}
?>
--EXPECT--
O:13:"ArrayIterator":4:{i:0;i:0;i:1;a:2:{i:3;s:1:"x";s:2:"id";i:7;}i:2;a:0:{}i:3;N;}
ArrayIterator|1
3|x|2
3=x
id=7

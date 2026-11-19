--TEST--
SplPriorityQueue serialization bytes and restored state
--FILE--
<?php
$original = new SplPriorityQueue;
$original->insert('later', 2);
$original->insert('first', 9);
$original->setExtractFlags(SplPriorityQueue::EXTR_BOTH);
$wire = serialize($original);
echo $wire, "\n";
$copy = unserialize($wire);
echo get_class($copy), '|', (int) (serialize($copy) === $wire), "\n";
while (!$copy->isEmpty()) {
    $entry = $copy->extract();
    echo $entry['data'], '|', $entry['priority'], "\n";
}
?>
--EXPECT--
O:16:"SplPriorityQueue":2:{i:0;a:0:{}i:1;a:2:{s:5:"flags";i:3;s:13:"heap_elements";a:2:{i:0;a:2:{s:4:"data";s:5:"first";s:8:"priority";i:9;}i:1;a:2:{s:4:"data";s:5:"later";s:8:"priority";i:2;}}}}
SplPriorityQueue|1
first|9
later|2

--TEST--
SplHeap serialization bytes and restored state
--FILE--
<?php
class RoundTripHeap extends SplHeap {
    protected function compare(mixed $value1, mixed $value2): int {
        return $value1 <=> $value2;
    }
}
$original = new RoundTripHeap;
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
O:13:"RoundTripHeap":2:{i:0;a:0:{}i:1;a:2:{s:5:"flags";i:0;s:13:"heap_elements";a:4:{i:0;i:9;i:1;i:3;i:2;i:5;i:3;i:1;}}}
RoundTripHeap|1
9
5
3
1

--TEST--
Heap insertion and destructive foreach call a protected comparator override
--FILE--
<?php
class Smallest extends SplMaxHeap {
    protected function compare(mixed $a, mixed $b): int { return $b <=> $a; }
}
$heap = new Smallest;
foreach ([4, 1, 3, 2] as $value) {
    $heap->insert($value);
}
$copy = clone $heap;
foreach ($heap as $key => $value) {
    echo $key, '=', $value, "\n";
}
var_dump($heap->count(), $copy->top(), $copy->count());
--EXPECT--
3=1
2=2
1=3
0=4
int(0)
int(1)
int(4)

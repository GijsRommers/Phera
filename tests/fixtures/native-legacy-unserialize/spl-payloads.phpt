--TEST--
Native legacy payloads initialize SPL objects and respect userland overrides
--FILE--
<?php
class CustomCollection extends ArrayObject {
    public function unserialize(string $data): void { echo "override\n"; }
}
$lists = [new ArrayObject(['key' => 10]), new ArrayIterator([20]), new SplDoublyLinkedList(), new SplObjectStorage(), new CustomCollection([30])];
$lists[2]->push(5);
foreach ($lists as $list) {
    $class = $list::class;
    $payload = $list->serialize();
    $wire = sprintf('C:%d:"%s":%d:{%s}', strlen($class), $class, strlen($payload), $payload);
    $result = unserialize($wire);
    echo $result::class, ':', count($result), "\n";
}
--EXPECT--
ArrayObject:1
ArrayIterator:1
SplDoublyLinkedList:1
SplObjectStorage:0
override
CustomCollection:0

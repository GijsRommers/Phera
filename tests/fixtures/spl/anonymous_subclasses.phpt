--TEST--
Anonymous SPL subclasses keep native dimensions and PHP overrides
--FILE--
<?php
$array = new class (['one' => 1, 'two' => 2]) extends ArrayObject {
    public function offsetGet(mixed $key): mixed {
        return 10 * parent::offsetGet($key);
    }
};
$array['three'] = 3;
echo $array['two'], ' ', count($array), ' ', implode(',', $array->getArrayCopy()), "\n";

$storage = new class extends SplObjectStorage {
    public function getHash(object $object): string {
        return $object->key;
    }
};
$first = (object) ['key' => 'same'];
$second = (object) ['key' => 'same'];
$storage[$first] = 'first';
$storage[$second] = 'second';
echo count($storage), ' ', $storage[$first], ' ', $storage[$second], "\n";
--EXPECT--
20 3 1,2,3
1 second second

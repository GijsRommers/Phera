--TEST--
SPL legacy serialization preserves arrays, list order and repeated object identity
--FILE--
<?php
$a = new ArrayObject(['key' => [1, true, null, 'value']]);
$serialized = $a->serialize();
$copy = new ArrayObject;
$copy->unserialize($serialized);
var_dump($serialized, $copy->getArrayCopy());

$q = new SplQueue;
$q->enqueue('first');
$q->enqueue(8);
$copy = new SplQueue;
$copy->unserialize($q->serialize());
var_dump($copy->dequeue(), $copy->dequeue());

$s = new SplObjectStorage;
$key = (object) ['name' => 'row'];
$s[$key] = ['same' => $key];
$copy = new SplObjectStorage;
$copy->unserialize($s->serialize());
foreach ($copy as $object) {
    var_dump($object->name, $copy[$object]['same'] === $object);
}
--EXPECT--
string(75) "x:i:0;a:1:{s:3:"key";a:4:{i:0;i:1;i:1;b:1;i:2;N;i:3;s:5:"value";}};m:a:0:{}"
array(1) {
  ["key"]=>
  array(4) {
    [0]=>
    int(1)
    [1]=>
    bool(true)
    [2]=>
    NULL
    [3]=>
    string(5) "value"
  }
}
string(5) "first"
int(8)
string(3) "row"
bool(true)

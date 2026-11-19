--TEST--
SplObjectStorage dimensions use overridden hashes and retain null information
--FILE--
<?php
class Records extends SplObjectStorage {
    public function getHash(object $object): string { return $object->name; }
}
$records = new Records;
$first = (object) ['name' => 'a'];
$other = (object) ['name' => 'a'];
$records[$first] = null;
var_dump(isset($records[$other]), $records[$other]);
$records[$other] = ['data' => [1, 2]];
foreach ($records as $key => $item) {
    var_dump($key, $item === $first, $records->getInfo());
}
$records->rewind();
$copy = clone $records;
$records->setInfo('changed');
var_dump($copy[$other], $records[$other], $records->count(COUNT_RECURSIVE));
--EXPECT--
bool(true)
NULL
int(0)
bool(true)
array(1) {
  ["data"]=>
  array(2) {
    [0]=>
    int(1)
    [1]=>
    int(2)
  }
}
array(1) {
  ["data"]=>
  array(2) {
    [0]=>
    int(1)
    [1]=>
    int(2)
  }
}
string(7) "changed"
int(1)

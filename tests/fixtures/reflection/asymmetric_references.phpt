--TEST--
Restricted setters protect existing reference aliases and nested static writes
--FILE--
<?php
class Record {
    public private(set) static array $shared = ['x' => 1];
    public private(set) array $value;
    public function __construct(array &$value) { $this->value =& $value; }
}
$value = ['x' => 2];
$record = new Record($value);
foreach ([
    fn () => Record::$shared['x']++,
    function () { unset(Record::$shared['x']); },
    function () use ($record) { $reference =& $record->value; },
    function () use ($record) { $reference =& $record->value['x']; },
] as $change) {
    try { $change(); } catch (Error $error) { echo $error->getMessage(), "\n"; }
}
var_dump($record->value, Record::$shared);
--EXPECT--
Cannot indirectly modify private(set) property Record::$shared from global scope
Cannot indirectly modify private(set) property Record::$shared from global scope
Cannot indirectly modify private(set) property Record::$value from global scope
Cannot indirectly modify private(set) property Record::$value from global scope
array(1) {
  ["x"]=>
  int(2)
}
array(1) {
  ["x"]=>
  int(1)
}

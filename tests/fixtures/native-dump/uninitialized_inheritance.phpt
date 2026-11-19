--TEST--
Dump uninitialized Exception slots while preserving DateTime and ArrayObject debug views
--FILE--
<?php
class ExtendedException extends Exception {
    public int $first;
    protected string $present = 'exception';
    private ?int $last;
}
class ExtendedArray extends ArrayObject {
    private int $first;
    public string $present = 'array';
    protected ?int $last;
}
class ExtendedDate extends DateTime {
    protected int $first;
    public string $present = 'date';
    public ?int $last;
}
foreach ([new ExtendedException('test'), new ExtendedArray([1]), new ExtendedDate('2024-01-01', new DateTimeZone('UTC'))] as $object) {
    var_dump($object);
    echo json_encode(print_r($object, true), JSON_UNESCAPED_SLASHES), "\n", json_encode(var_export($object, true), JSON_UNESCAPED_SLASHES), "\n";
}

--EXPECTF--
object(ExtendedException)#1 (8) {
  ["message":protected]=>
  string(4) "test"
  ["string":"Exception":private]=>
  string(0) ""
  ["code":protected]=>
  int(0)
  ["file":protected]=>
  string(%d) "%s"
  ["line":protected]=>
  int(17)
  ["trace":"Exception":private]=>
  array(0) {
  }
  ["previous":"Exception":private]=>
  NULL
  ["first"]=>
  uninitialized(int)
  ["present":protected]=>
  string(9) "exception"
  ["last":"ExtendedException":private]=>
  uninitialized(?int)
}
"ExtendedException Object\n(\n    [message:protected] => test\n    [string:Exception:private] => \n    [code:protected] => 0\n    [file:protected] => %s\n    [line:protected] => 17\n    [trace:Exception:private] => Array\n        (\n        )\n\n    [previous:Exception:private] => \n    [present:protected] => exception\n)\n"
"\\ExtendedException::__set_state(array(\n   'message' => 'test',\n   'string' => '',\n   'code' => 0,\n   'file' => '%s',\n   'line' => 17,\n   'trace' => \n  array (\n  ),\n   'previous' => NULL,\n   'present' => 'exception',\n))"
object(ExtendedArray)#2 (2) {
  ["present"]=>
  string(5) "array"
  ["storage":"ArrayObject":private]=>
  array(1) {
    [0]=>
    int(1)
  }
}
"ExtendedArray Object\n(\n    [present] => array\n    [storage:ArrayObject:private] => Array\n        (\n            [0] => 1\n        )\n\n)\n"
"\\ExtendedArray::__set_state(array(\n   0 => 1,\n))"
object(ExtendedDate)#3 (4) {
  ["present"]=>
  string(4) "date"
  ["date"]=>
  string(26) "2024-01-01 00:00:00.000000"
  ["timezone_type"]=>
  int(3)
  ["timezone"]=>
  string(3) "UTC"
}
"ExtendedDate Object\n(\n    [present] => date\n    [date] => 2024-01-01 00:00:00.000000\n    [timezone_type] => 3\n    [timezone] => UTC\n)\n"
"\\ExtendedDate::__set_state(array(\n   'present' => 'date',\n   'date' => '2024-01-01 00:00:00.000000',\n   'timezone_type' => 3,\n   'timezone' => 'UTC',\n))"

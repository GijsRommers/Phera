--TEST--
A method that acquires a dynamic local and returns a foreach reference while its caller prepares another call
--FILE--
<?php
class ReleaseMarker {
    public function __destruct() { echo "released\n"; }
}
class Reader {
    public function read($values, $name) {
        $$name = new ReleaseMarker;
        foreach ($values as &$element) {
            return $element;
        }
    }
}
function collect($first, $second) { return [$first, $second]; }
function callReader($reader, $values) {
    $call = 'collect';
    return $call('first', $reader->read($values, 'owner'));
}
$value = 4;
$values = [&$value];
$result = callReader(new Reader, $values);
$value = 9;
var_dump($result, $values[0]);
--EXPECT--
released
array(2) {
  [0]=>
  string(5) "first"
  [1]=>
  int(4)
}
int(9)

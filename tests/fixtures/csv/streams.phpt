--TEST--
String and stream CSV share quoting rules
--FILE--
<?php
$file = fopen('php://memory', 'w+');
fputcsv($file, ["a,b", "first\nsecond", 'a"b'], escape: '');
rewind($file);
$row = fgetcsv($file, escape: '');
rewind($file);
var_dump($row === str_getcsv(stream_get_contents($file), escape: ''), $row);
--EXPECT--
bool(true)
array(3) {
  [0]=>
  string(3) "a,b"
  [1]=>
  string(12) "first
second"
  [2]=>
  string(3) "a"b"
}

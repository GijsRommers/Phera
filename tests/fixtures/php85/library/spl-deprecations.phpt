--TEST--
SPL function and CSV method deprecations retain their behavior
--FILE--
<?php
set_error_handler(function ($severity, $message) { echo $severity, ': ', $message, PHP_EOL; });
var_dump(isset(spl_classes()['ArrayIterator']));
$object = new stdClass;
var_dump(strlen(spl_object_hash($object)));
$file = new SplTempFileObject;
$file->setCsvControl(';', '"', '');
var_dump($file->getCsvControl());
var_dump($file->fputcsv(['a', 'b'], ';', '"', ''));
$file->rewind();
var_dump($file->fgetcsv(';', '"', ''));
--EXPECT--
bool(true)
int(32)
array(3) {
  [0]=>
  string(1) ";"
  [1]=>
  string(1) """
  [2]=>
  string(0) ""
}
int(4)
array(2) {
  [0]=>
  string(1) "a"
  [1]=>
  string(1) "b"
}

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
8192: Function spl_classes() is deprecated since 8.6, use ReflectionExtension::getClassNames() instead
bool(true)
8192: Function spl_object_hash() is deprecated since 8.6, consider using spl_object_id() instead
int(32)
8192: Method SplFileObject::setCsvControl() is deprecated since 8.6
8192: Method SplFileObject::getCsvControl() is deprecated since 8.6
array(3) {
  [0]=>
  string(1) ";"
  [1]=>
  string(1) """
  [2]=>
  string(0) ""
}
8192: Method SplFileObject::fputcsv() is deprecated since 8.6
int(4)
8192: Method SplFileObject::fgetcsv() is deprecated since 8.6
array(2) {
  [0]=>
  string(1) "a"
  [1]=>
  string(1) "b"
}

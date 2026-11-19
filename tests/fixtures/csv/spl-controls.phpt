--TEST--
The SPL methods and READ_CSV use the stored controls
--FILE--
<?php
$f = new SplTempFileObject();
$f->setCsvControl(';', '"', '');
var_dump($f->getCsvControl(), $f->fputcsv(["a\nb", 'c"d']));
$f->rewind();
var_dump($f->fgetcsv());
$f->rewind();
$f->setFlags(SplFileObject::READ_CSV);
var_dump($f->current());
--EXPECT--
array(3) {
  [0]=>
  string(1) ";"
  [1]=>
  string(1) """
  [2]=>
  string(0) ""
}
int(13)
array(2) {
  [0]=>
  string(3) "a
b"
  [1]=>
  string(3) "c"d"
}
array(2) {
  [0]=>
  string(3) "a
b"
  [1]=>
  string(3) "c"d"
}

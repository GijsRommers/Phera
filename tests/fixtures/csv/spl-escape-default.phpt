--TEST--
SPL CSV deprecations distinguish default escape from stored controls
--FILE--
<?php
$f = new SplTempFileObject();
$f->fputcsv(['one']);
$f->rewind();
$f->fgetcsv();
$f->rewind();
$f->setFlags(SplFileObject::READ_CSV);
var_dump($f->current());
$f->setCsvControl(escape: '');
$f->fputcsv(['two']);
$f->setCsvControl();
$f->rewind();
var_dump($f->fgetcsv());
--EXPECTF--
Deprecated: SplFileObject::fputcsv(): the $escape parameter must be provided, as its default value will change, either explicitly or via SplFileObject::setCsvControl() in %s/spl-escape-default.php on line 3

Deprecated: SplFileObject::fgetcsv(): the $escape parameter must be provided, as its default value will change, either explicitly or via SplFileObject::setCsvControl() in %s/spl-escape-default.php on line 5
array(1) {
  [0]=>
  string(3) "one"
}

Deprecated: SplFileObject::setCsvControl(): the $escape parameter must be provided as its default value will change in %s/spl-escape-default.php on line 11
array(1) {
  [0]=>
  string(3) "one"
}

--TEST--
SPL validates multibyte controls and prints NUL controls as empty strings
--FILE--
<?php
$f = new SplTempFileObject();
foreach ([
    fn() => $f->setCsvControl("é", escape: ''),
    fn() => $f->fgetcsv(escape: "é"),
    fn() => $f->fputcsv([], enclosure: "é"),
] as $call) {
    try { $call(); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
}
$f->setCsvControl("\0", "\0", "\0");
var_dump($f->getCsvControl());
--EXPECT--
SplFileObject::setCsvControl(): Argument #1 ($separator) must be a single character
SplFileObject::fgetcsv(): Argument #3 ($escape) must be empty or a single character
SplFileObject::fputcsv(): Argument #3 ($enclosure) must be a single character
array(3) {
  [0]=>
  string(0) ""
  [1]=>
  string(0) ""
  [2]=>
  string(0) ""
}

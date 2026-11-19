--TEST--
SplTempFileObject shares its stream between byte operations and CSV line iteration
--FILE--
<?php
$file = new SplTempFileObject(32);
$file->fwrite("id;label\n1;\"two\nlines\"\n2;last");
$file->setCsvControl(';', '"', '');
$file->setFlags(SplFileObject::READ_CSV | SplFileObject::READ_AHEAD);
foreach ($file as $key => $row) {
    echo $key, ' ', json_encode($row), "\n";
}

$file->setFlags(SplFileObject::DROP_NEW_LINE);
$file->seek(0);
echo $file->key(), ' ', $file->current(), "\n";
$file->fseek(0);
echo $file->fgetc(), ' ', $file->ftell(), "\n";
try {
    clone $file;
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
0 ["id","label"]
1 ["1","two\nlines"]
2 ["2","last"]
0 id;label
i 1
Trying to clone an uncloneable object of class SplTempFileObject

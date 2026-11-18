<?php
mkdir('d');
file_put_contents('d/x', '');
var_dump(scandir('d'), scandir('d', SCANDIR_SORT_DESCENDING));
$d = dir('d');
var_dump($d->path, get_resource_type($d->handle), $d->read());
$d->rewind();
var_dump($d->read(), readdir($d->handle));
$d->close();
var_dump(is_resource($d->handle), get_resource_type($d->handle));
try {
    $d->read();
} catch (Throwable $error) {
    echo $error->getMessage(), "\n";
}
try {
    $d->path = 'changed';
} catch (Throwable $error) {
    echo $error->getMessage(), "\n";
}
$raw = opendir('d');
var_dump(readdir($raw));
rewinddir($raw);
var_dump(readdir($raw), fstat($raw), stream_get_meta_data($raw));
$id = get_resource_id($raw);
closedir($raw);
var_dump(get_resource_id($raw) === $id, gettype($raw));

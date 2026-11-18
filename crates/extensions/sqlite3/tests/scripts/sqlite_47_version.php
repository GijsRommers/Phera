<?php
$d=new SQLite3(':memory:');$version=SQLite3::version();
var_dump(array_keys($version),is_int($version['versionNumber']),
    $version['versionString']===$d->querySingle('SELECT sqlite_version()'));

<?php
$d=new SQLite3(':memory:');
$r=$d->query('SELECT 42 x');
var_dump($d->lastErrorCode(),$d->lastErrorMsg());
for($i=0;
$i<2;
$i++) {
    var_dump($r->fetchArray(),$d->lastErrorCode(),
    $d->lastErrorMsg());
}
$r->reset();
var_dump($d->lastErrorCode(),$d->lastErrorMsg());

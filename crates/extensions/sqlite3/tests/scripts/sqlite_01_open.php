<?php
$d=new SQLite3(':memory:');
var_dump($d->querySingle('SELECT 42'));
var_dump($d->close());
$d->open(':memory:');
var_dump($d->querySingle('SELECT 7'));

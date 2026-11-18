<?php
$d=new SQLite3(':memory:');
var_dump($d->querySingle("SELECT json_extract('{\"a\":42}','$.a')"));

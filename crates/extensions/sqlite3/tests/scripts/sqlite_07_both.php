<?php
$d=new SQLite3(':memory:');
$r=$d->query('SELECT 1 AS a,2 AS b');
var_dump($r->fetchArray(),$r->fetchArray());

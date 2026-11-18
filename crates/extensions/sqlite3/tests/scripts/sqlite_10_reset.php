<?php
$d=new SQLite3(':memory:');
$r=$d->query('SELECT 1 AS a UNION ALL SELECT 2');
var_dump($r->fetchArray(),$r->reset(),
$r->fetchArray(),$r->fetchArray(), $r->fetchArray());

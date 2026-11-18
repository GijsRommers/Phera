<?php
$d=new SQLite3(':memory:');
$s=$d->prepare('SELECT 1 UNION ALL SELECT 2');
$r=$s->execute();
var_dump($r->fetchArray(),$s->reset(),
$r->fetchArray());

<?php
$d=new SQLite3(':memory:');
$s=$d->prepare('SELECT 1 UNION ALL SELECT 2');
var_dump($s->busy());
$r=$s->execute();
var_dump($s->busy());
$r->fetchArray();
var_dump($s->busy());
$r->finalize();
var_dump($s->busy());

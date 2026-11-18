<?php
$d=new SQLite3(':memory:');
$s=$d->prepare('SELECT :x,typeof(:x)');
$v=42;
var_dump($s->bindValue(':x',$v,SQLITE3_INTEGER));
$v=99;
var_dump($s->execute()->fetchArray(SQLITE3_NUM));

<?php
$d=new SQLite3(':memory:');
$s=$d->prepare('SELECT :x AS a');
$s->bindValue('x',"O'Reilly");
var_dump($s->getSQL(),$s->getSQL(true));
$r=$s->execute();
var_dump($s->getSQL(true));

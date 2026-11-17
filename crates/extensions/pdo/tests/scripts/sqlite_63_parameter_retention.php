<?php
$p=new PDO('sqlite::memory:');
$s=$p->prepare('SELECT ? AS a,? AS b');
$s->execute([1,2]);
var_dump($s->fetch(PDO::FETCH_NUM));
$s->execute([3]);
var_dump($s->fetch(PDO::FETCH_NUM));
$s->execute([]);
var_dump($s->fetch(PDO::FETCH_NUM));

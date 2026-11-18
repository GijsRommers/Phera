<?php
$p=new PDO('sqlite::memory:');
$p->exec('CREATE TABLE a(id INTEGER PRIMARY KEY, v)');
$s=$p->prepare('INSERT INTO a(v) VALUES(1),(2) RETURNING id,v');
var_dump($s->execute(),$s->rowCount(),
$s->fetch(PDO::FETCH_NUM),$s->rowCount(),
$s->fetchAll(PDO::FETCH_NUM),$s->rowCount());

<?php
$p=new PDO('sqlite::memory:');
$p->exec('CREATE TABLE a(id INTEGER PRIMARY KEY, x TEXT)');
var_dump($p->lastInsertId());
$p->exec("INSERT INTO a(x) VALUES('a'),('b')");
var_dump($p->lastInsertId());
$p->exec('CREATE TABLE b(id TEXT PRIMARY KEY) WITHOUT ROWID');
$p->exec("INSERT INTO b VALUES('x')");
var_dump($p->lastInsertId(),$p->query('SELECT rowid,id,x FROM a')
    ->fetchAll(PDO::FETCH_NUM));

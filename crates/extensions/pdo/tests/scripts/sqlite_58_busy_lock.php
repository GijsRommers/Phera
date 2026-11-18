<?php
$p=new PDO('sqlite::memory:');
$p=new PDO('sqlite:'.$argv[1].'/busy.db');
$q=new PDO('sqlite:'.$argv[1].'/busy.db');
$p->exec('CREATE TABLE a(x);INSERT INTO a VALUES(1)');
$q->setAttribute(PDO::ATTR_TIMEOUT,0);
$p->exec('BEGIN IMMEDIATE;INSERT INTO a VALUES(2)');
var_dump($q->query('SELECT count(*) FROM a')->fetchColumn(),
$q->beginTransaction());
try{
    $q->exec('INSERT INTO a VALUES(3)');
}
catch(PDOException $e){
    var_dump($e->errorInfo);
}
$q->rollBack();
$p->rollBack();

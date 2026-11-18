<?php
$d=new SQLite3(':memory:');
$f=$argv[1].'/lock.db';
$a=new SQLite3($f);
$b=new SQLite3($f);
$b->enableExceptions(true);
$a->exec('CREATE TABLE a(x);BEGIN IMMEDIATE;INSERT INTO a VALUES(1)');
var_dump($b->busyTimeout(0));
try{
    $b->exec('INSERT INTO a VALUES(2)');
}
catch(Throwable $e){
    var_dump($e->getCode(),$e->getMessage());
}
$a->exec('ROLLBACK');

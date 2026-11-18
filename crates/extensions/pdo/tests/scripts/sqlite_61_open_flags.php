<?php
$p=new PDO('sqlite::memory:');
$file=$argv[1].'/flags.db';
$q=new PDO('sqlite:'.$file);
$q->exec('CREATE TABLE a(x)');
$r=new PDO('sqlite:'.$file,null,null,
[PDO::SQLITE_ATTR_OPEN_FLAGS=>PDO::SQLITE_OPEN_READONLY]);
try{
    $r->exec('INSERT INTO a VALUES(1)');
}
catch(PDOException $e){
    var_dump($e->errorInfo);
}

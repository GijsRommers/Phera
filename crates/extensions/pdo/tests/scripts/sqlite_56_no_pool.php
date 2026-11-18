<?php
$p=new PDO('sqlite::memory:');
$p->exec('CREATE TABLE a(x)');
$q=new PDO('sqlite::memory:');
try{
    $q->query('SELECT * FROM a');
}
catch(PDOException $e){
    var_dump($e->errorInfo);
}

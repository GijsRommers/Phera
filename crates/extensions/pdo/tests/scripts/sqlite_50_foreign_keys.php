<?php
$p=new PDO('sqlite::memory:');
foreach ([0,1] as $on){
    $p->exec("PRAGMA foreign_keys=$on");
    $p->exec('CREATE TABLE IF NOT EXISTS parent(id PRIMARY KEY); CREATE TABLE ' .
        'IF NOT EXISTS child(pid REFERENCES parent(id))');
    try{
        var_dump($p->exec('INSERT INTO child VALUES(99)'));
    }
    catch(PDOException $e){
        var_dump($e->getCode(),$e->errorInfo);
    }
}

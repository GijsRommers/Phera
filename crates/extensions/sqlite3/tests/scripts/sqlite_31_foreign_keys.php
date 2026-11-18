<?php
$d=new SQLite3(':memory:');
$d->enableExceptions(true);
foreach([0,1] as $on){
    $d->exec("PRAGMA foreign_keys=$on");
    $d->exec('CREATE TABLE IF NOT EXISTS p(id PRIMARY KEY);CREATE TABLE IF NOT ' .
        'EXISTS c(id REFERENCES p(id))');
    try{
        var_dump($d->exec('INSERT INTO c VALUES(99)'));
    }
    catch(Throwable $e){
        var_dump($e->getCode(),$e->getMessage());
    }
}

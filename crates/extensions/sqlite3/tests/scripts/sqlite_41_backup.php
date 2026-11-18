<?php
$d=new SQLite3(':memory:');
$e=new SQLite3(':memory:');
$d->exec("CREATE TABLE a(x);INSERT INTO a VALUES(42)");
var_dump($d->backup($e),$e->querySingle('SELECT x FROM a'));
$e->exec("UPDATE a SET x=7");
var_dump($d->querySingle('SELECT x FROM a'));
$d->enableExceptions(true);
try{
    $d->backup($d);
}
catch(Throwable $error){
    var_dump($error->getCode(),$error->getMessage());
}

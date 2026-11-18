<?php
$d=new SQLite3(':memory:');
$d->enableExceptions(true);
$d->exec('CREATE TABLE a(x UNIQUE);INSERT INTO a VALUES(1)');
foreach([false,true] as $on){
    var_dump($d->enableExtendedResultCodes($on));
    try{
        $d->exec('INSERT INTO a VALUES(1)');
    }
    catch(Throwable $e){
        var_dump($e->getCode(),$e->getMessage());
    }
    var_dump($d->lastErrorCode(),$d->lastExtendedErrorCode());
}

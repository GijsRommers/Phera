<?php
$d=new SQLite3(':memory:');
$d->enableExceptions(true);
try{
    $d->loadExtension('missing');
}
catch(Throwable $e){
    var_dump(get_class($e),$e->getMessage());
}

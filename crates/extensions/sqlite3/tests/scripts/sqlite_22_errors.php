<?php
$d=new SQLite3(':memory:');
$d->enableExceptions(true);
foreach(['exec','query','prepare','querySingle'] as $m){
    try{
        $r=$d->$m('SELECT missing');
    }
    catch(Throwable $e){
        var_dump(get_class($e),$e->getCode(),
        $e->getMessage());
    }
    var_dump($d->lastErrorCode(),$d->lastExtendedErrorCode(),
    $d->lastErrorMsg());
}

<?php
$d=new SQLite3(':memory:');
$r=$d->query('SELECT 1');
var_dump($r->finalize());
try{
    $r->fetchArray();
}
catch(Throwable $e){
    echo $e->getMessage(),"\n";
}

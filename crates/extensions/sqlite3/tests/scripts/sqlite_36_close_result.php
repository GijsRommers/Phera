<?php
$d=new SQLite3(':memory:');
$r=$d->query('SELECT 1');
var_dump($d->close());
try{
    $r->fetchArray();
}
catch(Throwable $e){
    echo $e->getMessage(),"\n";
}

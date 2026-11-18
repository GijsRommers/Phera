<?php
$d=new SQLite3(':memory:');
$s=$d->prepare('SELECT 1');
var_dump($s->close());
try{
    $s->execute();
}
catch(Throwable $e){
    echo $e->getMessage(),"\n";
}

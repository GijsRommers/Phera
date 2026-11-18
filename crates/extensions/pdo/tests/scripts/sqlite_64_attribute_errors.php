<?php
$p=new PDO('sqlite::memory:');
foreach([2,20,1000,1001,1002] as $a){
    try{
        var_dump($p->getAttribute($a));
    }
    catch(PDOException $e){
        var_dump($e->getCode(),$e->getMessage(),
        $e->errorInfo);
    }
}
var_dump($p->setAttribute(20,true),$p->setAttribute(1000,
1));

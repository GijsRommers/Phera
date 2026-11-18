<?php
$p=new PDO('sqlite::memory:');
$p->exec('CREATE TABLE a(i INTEGER PRIMARY KEY,t VARCHAR(20),b BLOB,r REAL)');
$p->exec("INSERT INTO a VALUES(1,'x',X'00FF',1.25)");
$s=$p->query('SELECT *,NULL AS n FROM a');
for($i=0;
$i<6;
$i++){
    try{
        var_dump($s->getColumnMeta($i));
    }
    catch(PDOException $e){
        var_dump($e->getCode(),$e->getMessage(),
        $e->errorInfo);
    }
}

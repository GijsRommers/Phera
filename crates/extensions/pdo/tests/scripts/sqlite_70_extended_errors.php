<?php
foreach([false,true] as $extended) {
    $p=PDO::connect('sqlite::memory:',
    null,null,[Pdo\Sqlite::ATTR_EXTENDED_RESULT_CODES=>$extended]);
    $p->exec('CREATE TABLE a(id INTEGER PRIMARY KEY,v UNIQUE NOT NULL ' .
        'CHECK(v>0));INSERT INTO a VALUES(1,1)');
    foreach(['INSERT INTO a VALUES(1,2)',
    'INSERT INTO a VALUES(2,1)', 'INSERT INTO a VALUES(2,NULL)',
    'INSERT INTO a VALUES(2,-1)'] as $sql) {
        try {
            $p->exec($sql);
        }
        catch(PDOException $e){
            var_dump($e->getCode(),$e->errorInfo);
        }
        try {
            $p->prepare($sql)->execute();
        }
        catch(PDOException $e){
            var_dump($e->getCode(),$e->errorInfo);
        }
    }
}

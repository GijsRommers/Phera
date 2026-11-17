<?php
$file=$argv[1].'/locks.db';
$p=PDO::connect('sqlite:'.$file);
$q=PDO::connect('sqlite:'.$file);
$p->setAttribute(PDO::ATTR_TIMEOUT,0);
$q->setAttribute(PDO::ATTR_TIMEOUT,0);
$p->exec('CREATE TABLE a(x);INSERT INTO a VALUES(1)');
foreach(['BEGIN','BEGIN IMMEDIATE','BEGIN EXCLUSIVE'] as $lock) {
    foreach([0,1,2] as $mode) {
        $p->exec($lock);
        var_dump($q->setAttribute(Pdo\Sqlite::ATTR_TRANSACTION_MODE,
        $mode));
        try {
            var_dump($q->beginTransaction(),
            $q->inTransaction());
        }
        catch(PDOException $e) {
            var_dump($e->getCode(),$e->errorInfo,
            $q->inTransaction());
        }
        if($q->inTransaction()) $q->rollBack();
        $p->rollBack();
    }
}

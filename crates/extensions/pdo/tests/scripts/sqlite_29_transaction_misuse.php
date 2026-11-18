<?php
// PHP 8.5 oracle: 29 transaction misuse.
$dsn='sqlite::memory:';
$p=new PDO($dsn,'phera','phera');
$t='phera_sqlite_29_transaction_misuse';
$p->exec("DROP TABLE IF EXISTS $t");
$p->exec("CREATE TABLE $t (id INTEGER PRIMARY KEY, label VARCHAR(80), score INT,
    amount DECIMAL(12,2), active BOOLEAN)");
try {
    $p->exec("INSERT INTO $t (label,score,amount,active) VALUES
        ('A',10,1.25,true),('B',20,2.50,false),('C',30,3.75,true)");
    foreach(['commit','rollBack'] as $method) {
        try {
            $p->$method();
        }
        catch(PDOException $e) {
            var_dump($e->getMessage());
        }
    }
    $p->beginTransaction();
    try {
        $p->beginTransaction();
    }
    catch(PDOException $e) {
        var_dump($e->getMessage());
    }
    $p->rollBack();
}
finally {
    if ($p->inTransaction()) {
        $p->rollBack();
    }
    try {
        $p->exec("DROP TABLE IF EXISTS $t");
    }
    catch (PDOException $e) {
        var_dump($e->getCode(), $e->getMessage(),
        $e->errorInfo);
        if (isset($s)) {
            $s->closeCursor();
        }
        $p->exec("DROP TABLE IF EXISTS $t");
    }
}

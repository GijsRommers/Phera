<?php
// PHP 8.5 oracle: 43 failed connection.
$dsn='sqlite::memory:';
$p=new PDO($dsn,'phera','phera');
$t='phera_sqlite_43_failed_connection';
$p->exec("DROP TABLE IF EXISTS $t");
$p->exec("CREATE TABLE $t (id INTEGER PRIMARY KEY, label VARCHAR(80), score INT,
    amount DECIMAL(12,2), active BOOLEAN)");
try {
    $p->exec("INSERT INTO $t (label,score,amount,active) VALUES
        ('A',10,1.25,true),('B',20,2.50,false),('C',30,3.75,true)");
    try {
        new PDO('sqlite:/nonexistent/phera/sqlite.db');
    }
    catch(PDOException $e) {
        var_dump(get_class($e),$e->getCode(),
        $e->getMessage(),$e->errorInfo);
    }
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

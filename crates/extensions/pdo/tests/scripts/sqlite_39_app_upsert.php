<?php
// PHP 8.5 oracle: 39 app upsert.
$dsn='sqlite::memory:';
$p=new PDO($dsn,'phera','phera');
$t='phera_sqlite_39_app_upsert';
$p->exec("DROP TABLE IF EXISTS $t");
$p->exec("CREATE TABLE $t (id INTEGER PRIMARY KEY, label VARCHAR(80), score INT,
    amount DECIMAL(12,2), active BOOLEAN)");
try {
    $p->exec("INSERT INTO $t (label,score,amount,active) VALUES
        ('A',10,1.25,true),('B',20,2.50,false),('C',30,3.75,true)");
    $s=$p->prepare("INSERT INTO $t (id,label,score) VALUES (1,?,?) " .
        "ON CONFLICT(id) DO UPDATE SET label=excluded.label,score=excluded.score");
    var_dump($s->execute(['Updated',
    88]));
    var_dump($p->query("SELECT id,label,score FROM $t WHERE id=1")
        ->fetch(PDO::FETCH_ASSOC));
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

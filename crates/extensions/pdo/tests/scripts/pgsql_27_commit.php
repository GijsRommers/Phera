<?php
// PHP 8.5 oracle: 27 commit.
$dsn='pgsql:host=127.0.0.1;port=54329;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$t='phera_pg_27_commit';
$p->exec("DROP TABLE IF EXISTS $t");
$p->exec("CREATE TABLE $t (id SERIAL PRIMARY KEY, label VARCHAR(80), score INT,
    amount DECIMAL(12,2), active BOOLEAN)");
try {
    $p->exec("INSERT INTO $t (label,score,amount,active) VALUES
        ('A',10,1.25,true),('B',20,2.50,false),('C',30,3.75,true)");
    var_dump($p->inTransaction(),$p->beginTransaction(),$p->inTransaction());
    $p->exec("UPDATE $t SET score=99 WHERE id=1");
    var_dump($p->commit(),$p->inTransaction(),
        $p->query("SELECT score FROM $t WHERE id=1")->fetchColumn());
 } finally {
    if ($p->inTransaction()) {$p->rollBack();}
    $p->exec("DROP TABLE IF EXISTS $t");
}

<?php
// PHP 8.5 oracle: 39 app upsert.
$dsn='pgsql:host=127.0.0.1;port=54329;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$t='phera_pg_39_app_upsert';
$p->exec("DROP TABLE IF EXISTS $t");
$p->exec("CREATE TABLE $t (id SERIAL PRIMARY KEY, label VARCHAR(80), score INT,
    amount DECIMAL(12,2), active BOOLEAN)");
try {
    $p->exec("INSERT INTO $t (label,score,amount,active) VALUES
        ('A',10,1.25,true),('B',20,2.50,false),('C',30,3.75,true)");
    $s=$p->prepare("INSERT INTO $t (id,label,score) VALUES (1,?,?) "
        . "ON CONFLICT (id) DO UPDATE SET label=excluded.label,score=excluded.score");
    var_dump($s->execute(['Updated',88]));
    var_dump($p->query("SELECT id,label,score FROM $t WHERE id=1")->fetch(PDO::FETCH_ASSOC));
 } finally {
    if ($p->inTransaction()) {$p->rollBack();}
    $p->exec("DROP TABLE IF EXISTS $t");
}

<?php
// PHP 8.5 oracle: 38 app pagination.
$dsn='pgsql:host=127.0.0.1;port=54329;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$t='phera_pg_38_app_pagination';
$p->exec("DROP TABLE IF EXISTS $t");
$p->exec("CREATE TABLE $t (id SERIAL PRIMARY KEY, label VARCHAR(80), score INT,
    amount DECIMAL(12,2), active BOOLEAN)");
try {
    $p->exec("INSERT INTO $t (label,score,amount,active) VALUES
        ('A',10,1.25,true),('B',20,2.50,false),('C',30,3.75,true)");
    $p->setAttribute(PDO::ATTR_EMULATE_PREPARES,false);
    $s=$p->prepare("SELECT id,label FROM $t ORDER BY id LIMIT ? OFFSET ?");
    $s->bindValue(1,2,PDO::PARAM_INT); $s->bindValue(2,1,PDO::PARAM_INT); $s->execute();
    var_dump($p->query("SELECT COUNT(*) FROM $t")->fetchColumn(),$s->fetchAll(PDO::FETCH_ASSOC));
 } finally {
    if ($p->inTransaction()) {$p->rollBack();}
    $p->exec("DROP TABLE IF EXISTS $t");
}

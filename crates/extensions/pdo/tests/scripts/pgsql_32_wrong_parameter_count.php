<?php
// PHP 8.5 oracle: 32 wrong parameter count.
$dsn='pgsql:host=127.0.0.1;port=54329;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$t='phera_pg_32_wrong_parameter_count';
$p->exec("DROP TABLE IF EXISTS $t");
$p->exec("CREATE TABLE $t (id SERIAL PRIMARY KEY, label VARCHAR(80), score INT,
    amount DECIMAL(12,2), active BOOLEAN)");
try {
    $p->exec("INSERT INTO $t (label,score,amount,active) VALUES
        ('A',10,1.25,true),('B',20,2.50,false),('C',30,3.75,true)");
    $p->setAttribute(PDO::ATTR_EMULATE_PREPARES,true);
    $s=$p->prepare("SELECT id FROM $t WHERE id=?");
    foreach([[],[1,2]] as $args) {
        try {$s->execute($args);} catch(PDOException $e) {var_dump($e->getMessage());}
        var_dump($s->errorCode(),$s->errorInfo());
    }
 } finally {
    if ($p->inTransaction()) {$p->rollBack();}
    $p->exec("DROP TABLE IF EXISTS $t");
}

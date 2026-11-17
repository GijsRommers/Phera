<?php
// PHP 8.5 oracle: 37 app rollback finally.
$dsn='mysql:host=127.0.0.1;port=33069;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$t='phera_my_37_app_rollback_finally';
$p->exec("DROP TABLE IF EXISTS $t");
$p->exec("CREATE TABLE $t (id INT AUTO_INCREMENT PRIMARY KEY, label VARCHAR(80), score INT,
    amount DECIMAL(12,2), active BOOLEAN)");
try {
    $p->exec("INSERT INTO $t (label,score,amount,active) VALUES
        ('A',10,1.25,true),('B',20,2.50,false),('C',30,3.75,true)");
    $p->beginTransaction();
    try {$p->exec("DELETE FROM $t"); throw new RuntimeException('abort');}
    catch (RuntimeException $e) {$p->rollBack(); echo $e->getMessage(),"\n";}
    finally {var_dump($p->inTransaction());}
    var_dump($p->query("SELECT COUNT(*) FROM $t")->fetchColumn());
 } finally {
    if ($p->inTransaction()) {$p->rollBack();}
    $p->exec("DROP TABLE IF EXISTS $t");
}

<?php
// PHP 8.5 oracle: 41 empty and reexecute.
$dsn='mysql:host=127.0.0.1;port=33069;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$t='phera_my_41_empty_and_reexecute';
$p->exec("DROP TABLE IF EXISTS $t");
$p->exec("CREATE TABLE $t (id INT AUTO_INCREMENT PRIMARY KEY, label VARCHAR(80), score INT,
    amount DECIMAL(12,2), active BOOLEAN)");
try {
    $p->exec("INSERT INTO $t (label,score,amount,active) VALUES
        ('A',10,1.25,true),('B',20,2.50,false),('C',30,3.75,true)");
    $s=$p->prepare("SELECT id FROM $t WHERE id>?"); $s->execute([99]);
    var_dump($s->fetch(),$s->fetchAll(),$s->columnCount()); $s->execute([1]);
    var_dump($s->fetchAll(PDO::FETCH_COLUMN));
 } finally {
    if ($p->inTransaction()) {$p->rollBack();}
    $p->exec("DROP TABLE IF EXISTS $t");
}

<?php
// PHP 8.5 oracle: 24 emulated types.
$dsn='mysql:host=127.0.0.1;port=33069;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$t='phera_my_24_emulated_types';
$p->exec("DROP TABLE IF EXISTS $t");
$p->exec("CREATE TABLE $t (id INT AUTO_INCREMENT PRIMARY KEY, label VARCHAR(80), score INT,
    amount DECIMAL(12,2), active BOOLEAN)");
try {
    $p->exec("INSERT INTO $t (label,score,amount,active) VALUES
        ('A',10,1.25,true),('B',20,2.50,false),('C',30,3.75,true)");
    $p->setAttribute(PDO::ATTR_EMULATE_PREPARES,true);
    $s=$p->prepare("SELECT id,amount,active FROM $t WHERE id=?"); $s->execute([1]);
    var_dump($s->fetch(PDO::FETCH_ASSOC));
 } finally {
    if ($p->inTransaction()) {$p->rollBack();}
    $p->exec("DROP TABLE IF EXISTS $t");
}

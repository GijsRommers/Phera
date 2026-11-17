<?php
// PHP 8.5 oracle: 47 charset.
$dsn='mysql:host=127.0.0.1;port=33069;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$t='phera_my_47_charset';
$p->exec("DROP TABLE IF EXISTS $t");
$p->exec("CREATE TABLE $t (id INT AUTO_INCREMENT PRIMARY KEY, label VARCHAR(80), score INT,
    amount DECIMAL(12,2), active BOOLEAN)");
try {
    $p->exec("INSERT INTO $t (label,score,amount,active) VALUES
        ('A',10,1.25,true),('B',20,2.50,false),('C',30,3.75,true)");
    $q=new PDO($dsn.';charset=utf8mb4','phera','phera');
    $s=$q->prepare("UPDATE $t SET label=? WHERE id=1");$s->execute(['é😀']);
    var_dump($q->query("SELECT label FROM $t WHERE id=1")->fetchColumn());
 } finally {
    if ($p->inTransaction()) {$p->rollBack();}
    $p->exec("DROP TABLE IF EXISTS $t");
}

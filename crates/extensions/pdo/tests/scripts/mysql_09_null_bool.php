<?php
// PHP 8.5 oracle: 09 null bool.
$dsn='mysql:host=127.0.0.1;port=33069;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$t='phera_my_09_null_bool';
$p->exec("DROP TABLE IF EXISTS $t");
$p->exec("CREATE TABLE $t (id INT AUTO_INCREMENT PRIMARY KEY, label VARCHAR(80), score INT,
    amount DECIMAL(12,2), active BOOLEAN)");
try {
    $p->exec("INSERT INTO $t (label,score,amount,active) VALUES
        ('A',10,1.25,true),('B',20,2.50,false),('C',30,3.75,true)");
    $s=$p->prepare("UPDATE $t SET active=?, label=? WHERE id=?");
    $s->bindValue(1, false, PDO::PARAM_BOOL); $s->bindValue(2, null, PDO::PARAM_NULL);
    $s->bindValue(3, 1, PDO::PARAM_INT); var_dump($s->execute());
    var_dump($p->query("SELECT active,label FROM $t WHERE id=1")->fetch(PDO::FETCH_NUM));
 } finally {
    if ($p->inTransaction()) {$p->rollBack();}
    $p->exec("DROP TABLE IF EXISTS $t");
}

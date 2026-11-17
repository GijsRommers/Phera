<?php
// PHP 8.5 oracle: 35 statement errors.
$dsn='mysql:host=127.0.0.1;port=33069;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$t='phera_my_35_statement_errors';
$p->exec("DROP TABLE IF EXISTS $t");
$p->exec("CREATE TABLE $t (id INT AUTO_INCREMENT PRIMARY KEY, label VARCHAR(80), score INT,
    amount DECIMAL(12,2), active BOOLEAN)");
try {
    $p->exec("INSERT INTO $t (label,score,amount,active) VALUES
        ('A',10,1.25,true),('B',20,2.50,false),('C',30,3.75,true)");
    $p->setAttribute(PDO::ATTR_ERRMODE,PDO::ERRMODE_SILENT);
    $s=$p->prepare("INSERT INTO $t (id,label) VALUES (?,?)");
    var_dump($s->execute([1,'duplicate']),$s->errorCode(),$s->errorInfo(),$p->errorCode());
 } finally {
    if ($p->inTransaction()) {$p->rollBack();}
    $p->exec("DROP TABLE IF EXISTS $t");
}

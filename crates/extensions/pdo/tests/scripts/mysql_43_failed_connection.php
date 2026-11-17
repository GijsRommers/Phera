<?php
// PHP 8.5 oracle: 43 failed connection.
$dsn='mysql:host=127.0.0.1;port=33069;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$t='phera_my_43_failed_connection';
$p->exec("DROP TABLE IF EXISTS $t");
$p->exec("CREATE TABLE $t (id INT AUTO_INCREMENT PRIMARY KEY, label VARCHAR(80), score INT,
    amount DECIMAL(12,2), active BOOLEAN)");
try {
    $p->exec("INSERT INTO $t (label,score,amount,active) VALUES
        ('A',10,1.25,true),('B',20,2.50,false),('C',30,3.75,true)");
    try {new PDO($dsn,'phera','wrong');}
    catch(PDOException $e) {var_dump(get_class($e),$e->getCode(),$e->getMessage(),$e->errorInfo);}
 } finally {
    if ($p->inTransaction()) {$p->rollBack();}
    $p->exec("DROP TABLE IF EXISTS $t");
}

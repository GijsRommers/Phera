<?php
// PHP 8.5 oracle: 34 warning mode.
$dsn='mysql:host=127.0.0.1;port=33069;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$t='phera_my_34_warning_mode';
$p->exec("DROP TABLE IF EXISTS $t");
$p->exec("CREATE TABLE $t (id INT AUTO_INCREMENT PRIMARY KEY, label VARCHAR(80), score INT,
    amount DECIMAL(12,2), active BOOLEAN)");
try {
    $p->exec("INSERT INTO $t (label,score,amount,active) VALUES
        ('A',10,1.25,true),('B',20,2.50,false),('C',30,3.75,true)");
    set_error_handler(function($n,$message) {echo $message,"\n"; return true;});
    $p->setAttribute(PDO::ATTR_ERRMODE,PDO::ERRMODE_WARNING);
    var_dump($p->exec("DELETE FROM ".$t."_missing")); restore_error_handler();
 } finally {
    if ($p->inTransaction()) {$p->rollBack();}
    $p->exec("DROP TABLE IF EXISTS $t");
}

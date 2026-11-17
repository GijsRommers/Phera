<?php
// PHP 8.5 oracle: 46 repeated names.
$dsn='mysql:host=127.0.0.1;port=33069;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$t='phera_my_46_repeated_names';
$p->exec("DROP TABLE IF EXISTS $t");
$p->exec("CREATE TABLE $t (id INT AUTO_INCREMENT PRIMARY KEY, label VARCHAR(80), score INT,
    amount DECIMAL(12,2), active BOOLEAN)");
try {
    $p->exec("INSERT INTO $t (label,score,amount,active) VALUES
        ('A',10,1.25,true),('B',20,2.50,false),('C',30,3.75,true)");
    foreach([false,true] as $emulate) {
        $p->setAttribute(PDO::ATTR_EMULATE_PREPARES,$emulate);
        $s=$p->prepare('SELECT :x + :x');
        try {var_dump($s->execute(['x'=>2]),$s->fetchColumn());}
        catch(PDOException $e) {var_dump($e->getMessage());}
    }
 } finally {
    if ($p->inTransaction()) {$p->rollBack();}
    $p->exec("DROP TABLE IF EXISTS $t");
}

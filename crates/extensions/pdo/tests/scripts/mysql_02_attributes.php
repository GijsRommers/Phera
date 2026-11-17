<?php
// PHP 8.5 oracle: 02 attributes.
$dsn='mysql:host=127.0.0.1;port=33069;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$t='phera_my_02_attributes';
$p->exec("DROP TABLE IF EXISTS $t");
$p->exec("CREATE TABLE $t (id INT AUTO_INCREMENT PRIMARY KEY, label VARCHAR(80), score INT,
    amount DECIMAL(12,2), active BOOLEAN)");
try {
    $p->exec("INSERT INTO $t (label,score,amount,active) VALUES
        ('A',10,1.25,true),('B',20,2.50,false),('C',30,3.75,true)");
    var_dump($p->getAttribute(PDO::ATTR_DRIVER_NAME), $p->getAttribute(PDO::ATTR_ERRMODE),
        $p->getAttribute(PDO::ATTR_EMULATE_PREPARES));
    var_dump($p->setAttribute(PDO::ATTR_DEFAULT_FETCH_MODE, PDO::FETCH_ASSOC));
    var_dump($p->query("SELECT id FROM $t ORDER BY id LIMIT 1")->fetch());
 } finally {
    if ($p->inTransaction()) {$p->rollBack();}
    $p->exec("DROP TABLE IF EXISTS $t");
}

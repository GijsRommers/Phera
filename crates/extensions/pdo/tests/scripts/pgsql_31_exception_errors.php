<?php
// PHP 8.5 oracle: 31 exception errors.
$dsn='pgsql:host=127.0.0.1;port=54329;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$t='phera_pg_31_exception_errors';
$p->exec("DROP TABLE IF EXISTS $t");
$p->exec("CREATE TABLE $t (id SERIAL PRIMARY KEY, label VARCHAR(80), score INT,
    amount DECIMAL(12,2), active BOOLEAN)");
try {
    $p->exec("INSERT INTO $t (label,score,amount,active) VALUES
        ('A',10,1.25,true),('B',20,2.50,false),('C',30,3.75,true)");
    try {$p->query("SELECT * FROM ".$t."_missing");}
    catch(PDOException $e) {var_dump($e->getMessage(),$e->getCode(),$e->errorInfo); }
 } finally {
    if ($p->inTransaction()) {$p->rollBack();}
    $p->exec("DROP TABLE IF EXISTS $t");
}

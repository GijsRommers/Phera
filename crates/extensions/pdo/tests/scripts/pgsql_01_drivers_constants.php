<?php
// PHP 8.5 oracle: 01 drivers constants.
$dsn='pgsql:host=127.0.0.1;port=54329;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$t='phera_pg_01_drivers_constants';
$p->exec("DROP TABLE IF EXISTS $t");
$p->exec("CREATE TABLE $t (id SERIAL PRIMARY KEY, label VARCHAR(80), score INT,
    amount DECIMAL(12,2), active BOOLEAN)");
try {
    $p->exec("INSERT INTO $t (label,score,amount,active) VALUES
        ('A',10,1.25,true),('B',20,2.50,false),('C',30,3.75,true)");
    var_dump(in_array('pgsql', PDO::getAvailableDrivers(), true),
        in_array('mysql', pdo_drivers(), true), PDO::PARAM_INT, PDO::FETCH_ASSOC,
        PDO::ERRMODE_EXCEPTION, PDO::ATTR_EMULATE_PREPARES);
 } finally {
    if ($p->inTransaction()) {$p->rollBack();}
    $p->exec("DROP TABLE IF EXISTS $t");
}

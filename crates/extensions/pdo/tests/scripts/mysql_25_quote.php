<?php
// PHP 8.5 oracle: 25 quote.
$dsn='mysql:host=127.0.0.1;port=33069;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$t='phera_my_25_quote';
$p->exec("DROP TABLE IF EXISTS $t");
$p->exec("CREATE TABLE $t (id INT AUTO_INCREMENT PRIMARY KEY, label VARCHAR(80), score INT,
    amount DECIMAL(12,2), active BOOLEAN)");
try {
    $p->exec("INSERT INTO $t (label,score,amount,active) VALUES
        ('A',10,1.25,true),('B',20,2.50,false),('C',30,3.75,true)");
    $value="O'Reilly\\path"; $quoted=$p->quote($value);
    var_dump($quoted,$p->query("SELECT $quoted")->fetchColumn());
    $statement = $p->prepare('SELECT ?');
    foreach (['NO_BACKSLASH_ESCAPES', ''] as $mode) {
        $p->exec("SET SESSION sql_mode = '$mode'");
        $statement->execute([$value]);
        var_dump($p->quote($value), $statement->fetchColumn() === $value);
        try {
            $p->query('SELECT +');
        } catch (PDOException $error) {
            var_dump($error->getCode());
        }
        var_dump($p->quote($value));
    }
 } finally {
    if ($p->inTransaction()) {$p->rollBack();}
    $p->exec("DROP TABLE IF EXISTS $t");
}

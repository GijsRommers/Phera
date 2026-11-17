<?php
$dsn='mysql:host=127.0.0.1;port=33069;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$p->exec("BEGIN");var_dump($p->inTransaction());
$p->exec("SAVEPOINT checkpoint");var_dump($p->inTransaction());
$p->exec("ROLLBACK TO SAVEPOINT checkpoint");var_dump($p->inTransaction());
$p->exec("COMMIT");var_dump($p->inTransaction());
$p->exec("/* leading */ START TRANSACTION");var_dump($p->inTransaction());
$p->exec("ROLLBACK");var_dump($p->inTransaction());
$p->beginTransaction();$p->exec("CREATE TEMPORARY TABLE trans_temp(id INT)");var_dump($p->inTransaction());$p->rollBack();
$p->beginTransaction();$p->exec("CREATE TABLE IF NOT EXISTS phera_pool_implicit(id INT)");var_dump($p->inTransaction());$p->exec("DROP TABLE phera_pool_implicit");
$p->beginTransaction();try {$p->exec("CREATE TABLE missing_database.bad(id INT)");} catch(Throwable $e) {} var_dump($p->inTransaction());

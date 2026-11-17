<?php
$dsn='pgsql:host=127.0.0.1;port=54329;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$p->exec("BEGIN");var_dump($p->inTransaction());
$p->exec("SAVEPOINT checkpoint");var_dump($p->inTransaction());
$p->exec("ROLLBACK TO SAVEPOINT checkpoint");var_dump($p->inTransaction());
$p->exec("COMMIT");var_dump($p->inTransaction());
$p->exec("/* leading */ START TRANSACTION");var_dump($p->inTransaction());
$p->exec("ROLLBACK");var_dump($p->inTransaction());
$p->exec("BEGIN; SELECT 1; COMMIT");var_dump($p->inTransaction());
$p->exec("BEGIN");$p->exec("COMMIT AND CHAIN");var_dump($p->inTransaction());$p->rollBack();

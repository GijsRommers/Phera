<?php
$p=new PDO('sqlite::memory:');
$p->exec('BEGIN');
var_dump($p->inTransaction());
$p->exec('SAVEPOINT a');
$p->exec('ROLLBACK TO a');
var_dump($p->inTransaction());
$p->exec('COMMIT');
var_dump($p->inTransaction());

<?php
$p=new PDO('sqlite::memory:');
$p=new PDO('sqlite:'.$argv[1].'/wal.db');
$p->exec('PRAGMA journal_mode=WAL; CREATE TABLE a(x)');
$q=new PDO('sqlite:'.$argv[1].'/wal.db');
$p->exec('BEGIN IMMEDIATE; INSERT INTO a VALUES(1)');
var_dump($q->query('SELECT count(*) FROM a')->fetchColumn());
$p->exec('COMMIT');
var_dump($q->query('SELECT count(*) FROM a')->fetchColumn());

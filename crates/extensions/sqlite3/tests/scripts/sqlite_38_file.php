<?php
$d=new SQLite3(':memory:');
$f=$argv[1].'/data.db';
$a=new SQLite3($f);
$a->exec('CREATE TABLE a(x);INSERT INTO a VALUES(42)');
$a->close();
$b=new SQLite3($f,SQLITE3_OPEN_READONLY);
var_dump($b->querySingle('SELECT x FROM a'));

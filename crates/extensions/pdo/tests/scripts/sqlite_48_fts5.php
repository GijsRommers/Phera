<?php
$p=new PDO('sqlite::memory:');
$p->exec('CREATE VIRTUAL TABLE docs USING fts5(body)');
$p->exec("INSERT INTO docs VALUES('hello world'),('hello sqlite')");
var_dump($p->query("SELECT rowid,body FROM docs WHERE docs MATCH 'sqlite'")
    ->fetchAll(PDO::FETCH_NUM));

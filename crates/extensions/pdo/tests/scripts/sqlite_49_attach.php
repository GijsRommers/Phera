<?php
$p=new PDO('sqlite::memory:');
$p->exec("ATTACH ':memory:' AS extra; CREATE TABLE extra.a(id); INSERT " .
    "INTO extra.a VALUES(7)");
var_dump($p->query('SELECT * FROM extra.a')->fetchAll(PDO::FETCH_NUM));
$p->exec('DETACH extra');

<?php
$p=new PDO('sqlite::memory:');
$q=PDO::connect('sqlite::memory:');
var_dump(get_class($q),$q instanceof PDO,
$q->query('SELECT 1')->fetchColumn());

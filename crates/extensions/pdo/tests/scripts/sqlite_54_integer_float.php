<?php
$p=new PDO('sqlite::memory:');
var_dump($p->query('SELECT ' .
    '-9223372036854775808,9223372036854775807,1.2345678901234567,1e300,1e-300,-0.0')
    ->fetch(PDO::FETCH_NUM));
$p->setAttribute(PDO::ATTR_STRINGIFY_FETCHES,
true);
var_dump($p->query('SELECT 9223372036854775807,1.2345678901234567,NULL')
    ->fetch(PDO::FETCH_NUM));

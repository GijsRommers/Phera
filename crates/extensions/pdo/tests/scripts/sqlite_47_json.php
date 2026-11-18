<?php
$p=new PDO('sqlite::memory:');
var_dump($p->query("SELECT json_extract('{\"a\":[1,2]}','$.a[1]'), " .
    "json_array(1,'x',NULL), json_valid('no'), json_type('null')")
    ->fetch(PDO::FETCH_NUM));

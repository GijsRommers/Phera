<?php
$d=new SQLite3(':memory:');
foreach(['SELECT 1 AS a,2 AS b','SELECT NULL',
'SELECT 1 WHERE 0',''] as $q)foreach([false,
true] as $entire)var_dump($d->querySingle($q,
$entire));

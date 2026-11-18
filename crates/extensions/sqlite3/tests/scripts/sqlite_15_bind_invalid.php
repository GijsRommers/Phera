<?php
$d=new SQLite3(':memory:');
$s=$d->prepare('SELECT :a,?');
foreach([0,-1,1,2,3,'','a',':a','missing',
'@a'] as $k)var_dump($s->bindValue($k,
1));

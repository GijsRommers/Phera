<?php
$p=new PDO('sqlite::memory:');
$p->exec('CREATE VIRTUAL TABLE boxes USING rtree(id,x1,x2,y1,y2);INSERT ' .
    'INTO boxes VALUES(1,0,2,0,2)');
var_dump($p->query('SELECT id,sqrt(4),pow(2,3) FROM boxes WHERE x1<=1 AND x2>=1')
    ->fetch(PDO::FETCH_NUM));

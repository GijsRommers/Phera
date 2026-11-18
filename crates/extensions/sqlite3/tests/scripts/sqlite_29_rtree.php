<?php
$d=new SQLite3(':memory:');
$d->exec('CREATE VIRTUAL TABLE boxes USING rtree(id,x1,x2,y1,y2);INSERT ' .
    'INTO boxes VALUES(1,0,2,0,2)');
var_dump($d->querySingle('SELECT id FROM boxes WHERE x1<=1 AND x2>=1'));

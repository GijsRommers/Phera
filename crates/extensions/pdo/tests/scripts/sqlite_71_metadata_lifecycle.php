<?php
$p=new PDO('sqlite::memory:');
$s=$p->prepare('SELECT 42 x');
foreach([0,1,2,3] as $stage) {
    if($stage==1) $s->execute();
    if($stage==2) $s->fetchAll();
    if($stage==3) $s->closeCursor();
    foreach([0,2] as $i) {
        try {
            var_dump($s->getColumnMeta($i));
        }
        catch(PDOException $e) {
            var_dump($e->errorInfo);
        }
    }
    var_dump($s->columnCount());
}

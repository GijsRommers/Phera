<?php
$d=new SQLite3(':memory:');
foreach(['SELECT 1','SELECT ?2,?1','SELECT :x,:x,@y,$z',
'SELECT ?, :a'] as $q){
    $s=$d->prepare($q);
    var_dump($s->paramCount(),$s->readOnly());
}

<?php
$d=new SQLite3(':memory:');
$s=$d->prepare('SELECT :a,@b,$c,?4');
foreach(['a',':a','@b','$c',3,4,0,-1,
''] as $key) {
    var_dump($s->bindValue($key,42,SQLITE3_INTEGER));
    $r=$s->execute();
    var_dump($r->fetchArray(SQLITE3_NUM));
}

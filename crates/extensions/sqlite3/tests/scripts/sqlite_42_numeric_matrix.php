<?php
$d=new SQLite3(':memory:');
$count=0;
foreach([SQLITE3_INTEGER,SQLITE3_FLOAT,
SQLITE3_TEXT,SQLITE3_BLOB,SQLITE3_NULL] as $type) {
    for($i=-50;
    $i<=50;
    $i++) {
        foreach([$i,$i/7,(string)$i] as $value) {
            $s=$d->prepare('SELECT ?,typeof(?)');
            $s->bindValue(1,$value,$type);
            $s->bindValue(2,$value,$type);
            var_dump($s->execute()->fetchArray(SQLITE3_NUM));
            $count++;
        }
    }
}
echo 'compared calls: ',$count,"\n";

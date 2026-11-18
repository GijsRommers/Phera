<?php
$d=new SQLite3(':memory:');
foreach([SQLITE3_INTEGER,SQLITE3_FLOAT,
SQLITE3_TEXT,SQLITE3_BLOB,SQLITE3_NULL] as $t){
    foreach([null,false,true,0,-1,42,
    PHP_INT_MAX,PHP_INT_MIN,1.25, "42",
    "1.25","abc","","a\0b","\xff"] as $v){
        $s=$d->prepare('SELECT ?,typeof(?)');
        $s->bindValue(1,$v,$t);
        $s->bindValue(2,$v,$t);
        var_dump($s->execute()->fetchArray(SQLITE3_NUM));
    }
}

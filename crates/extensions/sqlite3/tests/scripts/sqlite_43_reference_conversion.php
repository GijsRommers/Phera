<?php
$d=new SQLite3(':memory:');
foreach([SQLITE3_INTEGER,SQLITE3_FLOAT,
SQLITE3_TEXT,SQLITE3_BLOB,SQLITE3_NULL] as $type) {
    foreach([null,false,true,0,42,-3.5,
    '7','no',"a\0b"] as $input) {
        $value=$input;
        $s=$d->prepare('SELECT ?,typeof(?)');
        $s->bindParam(1,$value,$type);
        $s->bindParam(2,$value,$type);
        var_dump($value);
        $r=$s->execute();
        var_dump($value,$r->fetchArray(SQLITE3_NUM));
        $value=37;
        $r=$s->execute();
        var_dump($value,$r->fetchArray(SQLITE3_NUM));
    }
}

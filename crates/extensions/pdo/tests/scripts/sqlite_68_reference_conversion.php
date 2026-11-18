<?php
$p=new PDO('sqlite::memory:');
foreach([PDO::PARAM_INT,PDO::PARAM_STR,
PDO::PARAM_LOB,PDO::PARAM_BOOL, PDO::PARAM_NULL] as $type) {
    foreach([null,false,true,0,42,-3.5,
    '7','no',"a\0b"] as $input) {
        $value=$input;
        $s=$p->prepare('SELECT ?,typeof(?)');
        $s->bindParam(1,$value,$type);
        $s->bindParam(2,$value,$type);
        var_dump($value);
        $s->execute();
        var_dump($value,$s->fetch(PDO::FETCH_NUM));
        $value=37;
        $s->execute();
        var_dump($value,$s->fetch(PDO::FETCH_NUM));
    }
}

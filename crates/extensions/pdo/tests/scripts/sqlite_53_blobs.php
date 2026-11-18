<?php
$p=new PDO('sqlite::memory:');
foreach (["", "a\0b", "\x00\xff\xfe"] as $v){
    foreach([PDO::PARAM_STR,PDO::PARAM_LOB] as $t){
        $s=$p->prepare('SELECT ?,typeof(?)');
        $s->bindValue(1,$v,$t);
        $s->bindValue(2,$v,$t);
        $s->execute();
        var_dump($s->fetch(PDO::FETCH_NUM));
    }
}

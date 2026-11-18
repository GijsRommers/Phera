<?php
$p=new PDO('sqlite::memory:');
$count=0;
foreach([false,true] as $stringify) {
    $p->setAttribute(PDO::ATTR_STRINGIFY_FETCHES,
    $stringify);
    foreach([PDO::PARAM_INT,PDO::PARAM_STR,
    PDO::PARAM_LOB,PDO::PARAM_BOOL, PDO::PARAM_NULL] as $type) {
        for($i=-50;
        $i<=50;
        $i++) {
            foreach([$i,$i/7,(string)$i] as $value) {
                $s=$p->prepare('SELECT ?,typeof(?)');
                $s->bindValue(1,$value,
                $type);
                $s->bindValue(2,$value,
                $type);
                $s->execute();
                var_dump($s->fetch(PDO::FETCH_NUM));
                $count++;
            }
        }
    }
}
echo 'compared calls: ',$count,"\n";

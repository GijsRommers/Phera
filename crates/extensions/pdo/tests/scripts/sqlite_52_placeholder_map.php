<?php
$p=new PDO('sqlite::memory:');
foreach (['SELECT :x,:x','SELECT @x',
'SELECT $x','SELECT ?2,?1','SELECT ?,:x'] as $sql){
    foreach ([[7],['x'=>7],['@x'=>7],
    ['$x'=>7],[]] as $v){
        $s=$p->prepare($sql);
        try{
            var_dump($s->execute($v),
            $s->fetch(PDO::FETCH_NUM));
        }
        catch(PDOException $e){
            echo $e->getMessage(),"\n";
        }
    }
}

<?php
require __DIR__ . '/../support.php';
$r->rPush($p.'l','3','1','2');
foreach ([null,['sort'=>'desc'],['alpha'=>true],['limit'=>[1,1]],['get'=>'#'],
    ['store'=>$p.'dst']] as $options) {
    check(fn()=>$r->sort($p.'l',$options));
}
foreach (['sortAsc','sortDesc','sortAscAlpha','sortDescAlpha'] as $method) {
    check(fn()=>$r->$method($p.'l'));
}
check(fn()=>$r->sort_ro($p.'l',['sort'=>'desc']));
finish();

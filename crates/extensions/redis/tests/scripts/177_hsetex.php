<?php
require __DIR__ . '/../support.php';
foreach ([null,['EX'=>60],['PX'=>60000],['EXAT'=>4102444800],['PXAT'=>4102444800000],
    ['FNX','EX'=>60],['FXX','EX'=>60],['KEEPTTL']] as $options) {
    $r->del($p.'h');
    check(fn()=>$r->hsetex($p.'h',['a'=>"v\0",'b'=>'w'],$options));
    check(fn()=>$r->hsetex($p.'h',['a'=>'second'],$options));
    check(fn()=>$r->hGetAll($p.'h'));
}
finish();

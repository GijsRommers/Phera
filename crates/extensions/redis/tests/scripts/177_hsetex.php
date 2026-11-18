<?php
require __DIR__ . '/../support.php';
foreach ([null,['EX'=>60],['PX'=>60000],['EXAT'=>4102444800],['PXAT'=>4102444800000],
    ['FNX','EX'=>60],['FXX','EX'=>60],['KEEPTTL']] as $options) {
    $r->del($p.'h');
    check(fn()=>$r->hsetex($p.'h',['a'=>"v\0",'b'=>'w'],$options));
    check(fn()=>$r->hsetex($p.'h',['a'=>'second'],$options));
    // Hash-field expiry can change Redis's unordered hash-table iteration order.
    check(function()use($r,$p){$fields=$r->hGetAll($p.'h');ksort($fields);return $fields;});
}
finish();

<?php
require __DIR__ . '/../support.php';
foreach ([[],['NX'],['XX'],['CH'],['GT'],['LT'],['INCR'],['NX','CH'],['XX','GT','CH']] as $options) {
    $r->del($p.'z');
    check(fn()=>$r->zAdd($p.'z',$options,1,'a'));
    check(fn()=>$r->zAdd($p.'z',$options,2,'a'));
    check(fn()=>$r->zRange($p.'z',0,-1,true));
}
$r->del($p.'z');$r->zAdd($p.'z',1,'a',2,'b',3,'c');
foreach ([false,true,[],['withscores'=>true],['rev'],['byscore'],
    ['byscore','limit'=>[0,2],'withscores'=>true]] as $options) {
    check(fn()=>$r->zRange($p.'z',0,3,$options));
}
finish();

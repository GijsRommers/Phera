<?php
require __DIR__ . '/../support.php';
$r->setOption(Redis::OPT_SERIALIZER,Redis::SERIALIZER_JSON);
$r->setOption(Redis::OPT_PREFIX,$p);
foreach (["v\0x",['nested'=>[1,true,null]]] as $value) {
    $r->del('l','s','z','h','x');
    check(fn()=>$r->rPush('l',$value));check(fn()=>$r->lIndex('l',0));
    check(fn()=>$r->lRange('l',0,-1));check(fn()=>$r->lPop('l',1));
    check(fn()=>$r->sAdd('s',$value));check(fn()=>$r->sIsMember('s',$value));
    check(fn()=>$r->sMisMember('s',$value));check(fn()=>$r->sMembers('s'));
    check(fn()=>$r->sPop('s',1));
    check(fn()=>$r->zAdd('z',1.5,$value));check(fn()=>$r->zScore('z',$value));
    check(fn()=>$r->zRange('z',0,-1));
    // phpredis 6.3.0 exits 255 with truncated var_dump for array-valued score keys.
    // Exercise score maps with valid string keys; retain array values above.
    if (is_string($value)) {
        check(fn()=>$r->zRange('z',0,-1,true));check(fn()=>$r->zPopMin('z',1));
    }
    check(fn()=>$r->hMSet('h',['a'=>$value]));check(fn()=>$r->hMGet('h',['a','missing']));
    check(fn()=>$r->xAdd('x','1-0',['f'=>$value]));check(fn()=>$r->xRange('x','-','+'));
}
$r->setOption(Redis::OPT_PREFIX,'');
finish();

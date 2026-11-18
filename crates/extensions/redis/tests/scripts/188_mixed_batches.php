<?php
require __DIR__ . '/../support.php';
foreach (['multi','pipeline'] as $mode) {
    $r->setOption(Redis::OPT_PREFIX,$p.$mode.':');
    check(fn()=>$r->$mode());
    check(fn()=>$r->set('s',"a\0b",['EX'=>60]));
    check(fn()=>$r->get('s'));check(fn()=>$r->exists('s','missing'));
    check(fn()=>$r->hSet('h','f','v'));check(fn()=>$r->hGetAll('h'));
    check(fn()=>$r->rPush('l','v'));check(fn()=>$r->lPop('l',2));
    check(fn()=>$r->sAdd('set','member'));check(fn()=>$r->sMembers('set'));
    check(fn()=>$r->zAdd('z',1.5,'member'));check(fn()=>$r->zRange('z',0,-1,true));
    check(fn()=>$r->xAdd('x','1-0',['field'=>'value']));check(fn()=>$r->xRange('x','-','+'));
    check(fn()=>$r->pfAdd('pf',['a','b']));check(fn()=>$r->pfCount('pf'));
    check(fn()=>$r->geoAdd('geo',0,0,'member'));check(fn()=>$r->geoHash('geo','member'));
    check(fn()=>$r->eval('return redis.call("GET",KEYS[1])',['s'],1));
    check(fn()=>$r->script('EXISTS',str_repeat('0',40)));
    check(fn()=>$r->publish('channel','message'));check(fn()=>$r->ping());
    check(fn()=>$r->rawCommand('MEMORY','USAGE',$p.'missing'));check(fn()=>$r->exec());
}
$r->setOption(Redis::OPT_PREFIX,'');
finish();

<?php
require __DIR__ . '/../support.php';
$r->setOption(Redis::OPT_PREFIX,$p);
check(fn()=>$r->mset(['a'=>'1','b'=>'2']));check(fn()=>$r->mget(['a','b','missing']));
check(fn()=>$r->hSet('h','field','value'));check(fn()=>$r->hGetAll('h'));
check(fn()=>$r->sAdd('s','member'));check(fn()=>$r->sMembers('s'));
check(fn()=>$r->rPush('l','value'));check(fn()=>$r->lPop('l'));
check(fn()=>$r->zAdd('z',1,'member'));check(fn()=>$r->zRange('z',0,-1,true));
check(fn()=>$r->xAdd('x','1-0',['field'=>'value']));check(fn()=>$r->xRead(['x'=>'0-0']));
check(fn()=>$r->eval('return redis.call("GET",KEYS[1])',['a'],1));
check(fn()=>$r->copy('a','c',['replace'=>true]));check(fn()=>$r->get('c'));
$r->multi()->get('a')->hGetAll('h');check(fn()=>$r->exec());
finish();

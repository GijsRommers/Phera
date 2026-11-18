<?php
require __DIR__."/../support.php";
check(fn()=>$r->multi());check(fn()=>$r->set($p."s","v"));
check(fn()=>$r->get($p."s"));check(fn()=>$r->hSet($p."h","f","v"));
check(fn()=>$r->hGetAll($p."h"));check(fn()=>$r->rPush($p."l","a"));
check(fn()=>$r->lPop($p."l"));check(fn()=>$r->sAdd($p."s2","a"));
check(fn()=>$r->zAdd($p."z",1,"a"));check(fn()=>$r->zRange($p."z",0,-1,true));
check(fn()=>$r->xAdd($p."x","1-0",["f"=>"v"]));
check(fn()=>$r->xRange($p."x","-","+"));check(fn()=>$r->get($p."missing"));
check(fn()=>$r->hGet($p."s","wrong"));check(fn()=>$r->exec());
finish();

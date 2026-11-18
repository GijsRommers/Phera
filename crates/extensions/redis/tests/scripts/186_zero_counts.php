<?php
require __DIR__ . '/../support.php';
$r->rPush($p.'l','a','b','c','d');$r->sAdd($p.'s','member');
check(fn()=>$r->lPop($p.'l',0));check(fn()=>$r->rPop($p.'l',0));
check(fn()=>$r->lPop($p.'l',2));check(fn()=>$r->rPop($p.'l',2));
check(fn()=>$r->sPop($p.'s',0));check(fn()=>$r->sRandMember($p.'s',0));
check(fn()=>$r->sPop($p.'s',1));check(fn()=>$r->sRandMember($p.'s',1));
check(fn()=>$r->multi());check(fn()=>$r->exec());
check(fn()=>$r->pipeline());check(fn()=>$r->exec());
finish();

<?php
require __DIR__."/../support.php";
$other=new Redis();$other->connect("127.0.0.1",16399);$other->select(14);
$r->set($p."w","first");check(fn()=>$r->watch($p."w"));
$other->set($p."w","other");$r->multi();$r->get($p."w");check(fn()=>$r->exec());
check(fn()=>$r->unwatch());
finish();

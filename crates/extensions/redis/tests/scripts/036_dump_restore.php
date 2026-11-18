<?php
require __DIR__."/../support.php";
check(fn()=>$r->restore($p."b",0,$r->dump($p."a")));$r->set($p."a","v\0");check(fn()=>$r->restore($p."b",0,$r->dump($p."a")));
check(fn()=>$r->restore($p."b",0,$r->dump($p."a")));check(fn()=>$r->get($p."b"));
finish();

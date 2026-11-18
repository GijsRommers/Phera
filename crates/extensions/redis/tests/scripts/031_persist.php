<?php
require __DIR__."/../support.php";
check(fn()=>$r->persist($p."a"));$r->set($p."a","v\0");check(fn()=>$r->persist($p."a"));
check(fn()=>$r->persist($p."a"));check(fn()=>$r->get($p."b"));
finish();

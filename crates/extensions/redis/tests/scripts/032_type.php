<?php
require __DIR__."/../support.php";
check(fn()=>$r->type($p."a"));$r->set($p."a","v\0");check(fn()=>$r->type($p."a"));
check(fn()=>$r->type($p."a"));check(fn()=>$r->get($p."b"));
finish();

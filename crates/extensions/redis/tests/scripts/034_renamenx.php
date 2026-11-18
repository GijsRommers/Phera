<?php
require __DIR__."/../support.php";
check(fn()=>$r->renameNx($p."a",$p."b"));$r->set($p."a","v\0");check(fn()=>$r->renameNx($p."a",$p."b"));
check(fn()=>$r->renameNx($p."a",$p."b"));check(fn()=>$r->get($p."b"));
finish();

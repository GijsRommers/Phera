<?php
require __DIR__."/../support.php";
check(fn()=>$r->unlink($p."a",$p."b"));$r->set($p."a","v\0");check(fn()=>$r->unlink($p."a",$p."b"));
check(fn()=>$r->unlink($p."a",$p."b"));check(fn()=>$r->get($p."b"));
finish();

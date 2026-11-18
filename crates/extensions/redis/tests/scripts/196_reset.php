<?php
require __DIR__ . '/../support.php';
// reset() is sent at once, also during MULTI, which it ends, and the
// connection is back in database 0.
check(fn()=>$r->getDBNum());
check(fn()=>$r->reset());
check(fn()=>$r->getDBNum());
check(fn()=>$r->getMode());
$r->select(14);
$r->multi();
check(fn()=>$r->getMode());
check(fn()=>$r->reset());
check(fn()=>$r->getMode());
check(fn()=>$r->getDBNum());
$r->select(14);
finish();

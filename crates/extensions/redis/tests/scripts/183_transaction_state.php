<?php
require __DIR__ . '/../support.php';
foreach (['multi','pipeline'] as $mode) {
    $r->$mode();check(fn()=>$r->select(14));check(fn()=>$r->set($p.'k','v'));
    check(fn()=>$r->exec());check(fn()=>$r->getDBNum());
    $r->$mode();$r->set($p.'k','discarded');check(fn()=>$r->discard());
    check(fn()=>$r->get($p.'k'));check(fn()=>$r->getMode());
}
finish();

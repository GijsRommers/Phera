<?php
require __DIR__ . '/../support.php';
foreach (['multi','pipeline'] as $mode) {
    $r->set($p.'s','value');
    check(fn()=>$r->$mode());
    check(fn()=>$r->get($p.'s'));
    check(fn()=>$r->lPop($p.'s'));
    check(fn()=>$r->get($p.'missing'));
    check(fn()=>$r->exec());check(fn()=>$r->getLastError());$r->clearLastError();
    check(fn()=>$r->$mode());
    check(fn()=>$r->rawCommand('PHERA_INVALID_COMMAND'));
    check(fn()=>$r->get($p.'s'));
    check(fn()=>$r->exec());check(fn()=>$r->getMode());
    check(fn()=>$r->ping());check(fn()=>$r->getLastError());$r->clearLastError();
}
finish();

<?php
require __DIR__ . '/../support.php';
foreach ([Redis::SCAN_NORETRY,Redis::SCAN_RETRY,Redis::SCAN_PREFIX,Redis::SCAN_NOPREFIX] as $mode) {
    check(fn()=>$r->setOption(Redis::OPT_SCAN,$mode));check(fn()=>$r->getOption(Redis::OPT_SCAN));
}
foreach ([false,true] as $mode) {
    check(fn()=>$r->setOption(Redis::OPT_REPLY_LITERAL,$mode));
    check(fn()=>$r->ping());check(fn()=>$r->set($p.'s','v'));
    check(fn()=>$r->rawCommand('PING'));
    check(fn()=>$r->setOption(Redis::OPT_NULL_MULTIBULK_AS_NULL,$mode));
    check(fn()=>$r->blPop([$p.'missing'],0.01));
}
finish();

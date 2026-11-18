<?php
require __DIR__ . '/../support.php';
check(fn()=>$r->client('SETNAME','phera-oracle'));
check(fn()=>$r->client('GETNAME'));
check(fn()=>$r->config('GET','maxmemory-policy'));
check(fn()=>$r->command('COUNT') > 0);
check(fn()=>$r->dbSize() >= 0);
check(fn()=>count($r->time()));
check(fn()=>$r->wait(0,1));
check(fn()=>$r->rawCommand('MEMORY','USAGE',$p.'missing'));
check(fn()=>$r->object('ENCODING',$p.'missing'));
check(fn()=>$r->slowlog('LEN') >= 0);
check(fn()=>$r->info('server')['redis_version']);
finish();

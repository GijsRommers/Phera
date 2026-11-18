"""Generate family-scoped PHP scripts from explicit call tables; PHP is always the oracle."""
from pathlib import Path
root = Path(__file__).resolve().parent.parent
scripts = root/'tests/scripts'
cases = {}
def case(name, body):
    cases[name] = body
# One command family per script, with present, missing and wrong-type inputs.
strings = {
 'get': '$r->get($key)', 'getdel': '$r->getDel($key)',
 'getex': '$r->getEx($key, ["ex"=>60])', 'getset': '$r->getSet($key,"next\\0value")',
 'append': '$r->append($key,"tail\\0")', 'strlen': '$r->strlen($key)',
 'getrange': '$r->getRange($key,0,-1)', 'setrange': '$r->setRange($key,2,"xx")',
 'getbit': '$r->getBit($key,1)', 'setbit': '$r->setBit($key,1,true)',
 'bitcount': '$r->bitcount($key)', 'bitpos': '$r->bitpos($key,true)',
 'incr': '$r->incr($key)', 'incrby': '$r->incrBy($key,3)',
 'incrbyfloat': '$r->incrByFloat($key,0.5)', 'decr': '$r->decr($key)',
 'decrby': '$r->decrBy($key,3)',
}
for name,call in strings.items():
    numeric = name.startswith(('incr','decr'))
    initial = '"12"' if numeric else '"ab\\0cd"'
    case(name, f'''foreach (["present", "missing", "wrong"] as $kind) {{
$key=$p.$kind;
if ($kind === "present") {{ $r->set($key, {initial}); }}
if ($kind === "wrong") {{ $r->hSet($key,"f","v"); }}
check(fn()=>{call});
check(fn()=>$r->getLastError()); $r->clearLastError();
}}''')
for name,call in {
 'set': '$r->set($p."k",$value)', 'setex': '$r->setex($p."k",60,$value)',
 'psetex': '$r->psetex($p."k",60000,$value)', 'setnx': '$r->setnx($p."k",$value)',
 'mget': '$r->mget([$p."k",$p."missing"])',
 'mset': '$r->mset([$p."k"=>$value,$p."b"=>$value])',
 'msetnx': '$r->msetnx([$p."k"=>$value,$p."b"=>$value])',
}.items():
    case(name, f'''foreach (["", "ab\\0cd", "é", str_repeat("x",65536)] as $value) {{
$r->del($p."k",$p."b"); check(fn()=>{call});
check(fn()=>strlen($r->get($p."k"))); check(fn()=>{call});
}}''')
case('set_options', '''foreach ([null,60,["nx"],["xx"],["get"],["keepttl"],["ex"=>60],
["px"=>60000],["exat"=>4102444800],["pxat"=>4102444800000],
["nx","ex"=>60],["xx","get","px"=>60000]] as $options) {
$r->del($p."k"); check(fn()=>$r->set($p."k","first",$options));
check(fn()=>$r->set($p."k","second",$options)); check(fn()=>$r->get($p."k"));
}''')
case('bit_ranges', '''$r->set($p."k","ab\\0cd");
foreach ([0,1,8,-1] as $start) { foreach ([-1,3,15] as $end) {
foreach ([false,true] as $bits) {
check(fn()=>$r->bitcount($p."k",$start,$end,$bits));
check(fn()=>$r->bitpos($p."k",true,$start,$end,$bits));
}}}''')
case('bitop', '''$r->set($p."a","ab\\0");$r->set($p."b","cd\\0");
foreach (["AND","OR","XOR"] as $op) {check(fn()=>$r->bitop($op,$p."d",$p."a",$p."b"));
check(fn()=>bin2hex($r->get($p."d")));}
check(fn()=>$r->bitop("NOT",$p."d",$p."a"));''')
for name,call in {'del':'$r->del([$p."a",$p."b"])','unlink':'$r->unlink($p."a",$p."b")',
'exists':'$r->exists($p."a",$p."b")','touch':'$r->touch([$p."a",$p."b"])',
'persist':'$r->persist($p."a")','type':'$r->type($p."a")',
'rename':'$r->rename($p."a",$p."b")','renamenx':'$r->renameNx($p."a",$p."b")',
'copy':'$r->copy($p."a",$p."b",["REPLACE"])',
'dump_restore':'$r->restore($p."b",0,$r->dump($p."a"))'}.items():
    case(name,f'''check(fn()=>{call});$r->set($p."a","v\\0");check(fn()=>{call});
check(fn()=>{call});check(fn()=>$r->get($p."b"));''')
for name in ['expire','pexpire','expireAt','pexpireAt']:
    seconds = '4102444800000' if name=='pexpireAt' else '4102444800' if name=='expireAt' else '60000'
    case(name.lower(),f'''foreach ([null,"NX","XX","GT","LT"] as $mode) {{
$r->del($p."k"); check(fn()=>$r->{name}($p."k",{seconds},$mode));
$r->set($p."k","v");check(fn()=>$r->{name}($p."k",{seconds},$mode));
check(fn()=>$r->{name}($p."k",{seconds},$mode));}}''')
for name in ['ttl','pttl','expiretime','pexpiretime']:
    case(name,f'''check(fn()=>$r->{name}($p."k"));$r->set($p."k","v");
check(fn()=>$r->{name}($p."k"));$r->expireAt($p."k",4102444800);
check(fn()=>$r->{name}($p."missing"));''')
# Hash methods and independent field-expiry commands.
hashes = {
'hget':'$r->hGet($key,"a")','hgetall':'$r->hGetAll($key)', 'hkeys':'$r->hKeys($key)',
'hvals':'$r->hVals($key)','hlen':'$r->hLen($key)','hstrlen':'$r->hStrLen($key,"a")',
'hexists':'$r->hExists($key,"a")','hmget':'$r->hMget($key,["a","missing"])',
'hdel':'$r->hDel($key,"a","missing")','hset':'$r->hSet($key,"b","v\\0","c","z")',
'hmset':'$r->hMset($key,["b"=>"v\\0","c"=>"z"])','hsetnx':'$r->hSetNx($key,"b","v")',
'hincrby':'$r->hIncrBy($key,"a",2)','hincrbyfloat':'$r->hIncrByFloat($key,"a",0.5)',
'hrandfield':'$r->hRandField($key,["count"=>1,"withvalues"=>true])',
'hgetdel':'$r->hgetdel($key,["a","missing"])','hgetex':'$r->hgetex($key,["a","missing"],["EX"=>60])'}
for name,call in hashes.items():
    case(name,f'''foreach (["present","missing","wrong"] as $kind) {{
$key=$p.$kind;if ($kind==="present") {{$r->hSet($key,"a","12");}}
if ($kind==="wrong") {{$r->set($key,"wrong");}}
check(fn()=>{call});check(fn()=>$r->getLastError());$r->clearLastError();}}''')
for name in ['hexpire','hpexpire','hexpireat','hpexpireat']:
    case(name,f'''foreach ([null,"NX","XX","GT","LT"] as $mode) {{
$r->del($p."k");$r->hSet($p."k","a","12");
check(fn()=>$r->{name}($p."k",4102444800000,["a","missing"],$mode));
check(fn()=>$r->{name}($p."k",4102444800000,["a"],$mode));}}''')
for name in ['httl','hpttl','hexpiretime','hpexpiretime','hpersist']:
    case(name,f'''check(fn()=>$r->{name}($p."k",["a","missing"]));
$r->hSet($p."k","a","12");check(fn()=>$r->{name}($p."k",["a","missing"]));
$r->hpexpireat($p."k",4102444800000,["a"]);
check(fn()=>$r->{name}($p."k",["missing"]));''')
lists={
'llen':'$r->lLen($key)','lrange':'$r->lRange($key,0,-1)', 'lindex':'$r->lIndex($key,0)',
'lpop':'$r->lPop($key)','rpop':'$r->rPop($key)','lpush':'$r->lPush($key,"c","d")',
'rpush':'$r->rPush($key,"c","d")','lpushx':'$r->lPushx($key,"c")',
'rpushx':'$r->rPushx($key,"c")','lset':'$r->lSet($key,0,"c")',
'lrem':'$r->lRem($key,"a",1)','ltrim':'$r->lTrim($key,0,0)',
'linsert':'$r->lInsert($key,Redis::BEFORE,"a","c")',
'lpos':'$r->lPos($key,"a",["rank"=>1,"count"=>2])',
'lmove':'$r->lMove($key,$p."dst","LEFT","RIGHT")',
'rpoplpush':'$r->rPopLPush($key,$p."dst")',
'blmove':'$r->blmove($key,$p."dst","LEFT","RIGHT",0.01)',
'brpoplpush':'$r->brpoplpush($key,$p."dst",0.01)',
'blpop':'$r->blPop([$key],0.01)','brpop':'$r->brPop([$key],0.01)',
'lmpop':'$r->lmpop([$key],"LEFT",2)','blmpop':'$r->blmpop(0.01,[$key],"RIGHT",2)',
}
for name,call in lists.items():
    case(name,f'''foreach (["present","missing","wrong"] as $kind) {{
$key=$p.$kind;if ($kind==="present") {{$r->rPush($key,"a","b\\0");}}
if ($kind==="wrong") {{$r->set($key,"wrong");}}
check(fn()=>{call});check(fn()=>$r->getLastError());$r->clearLastError();}}''')
sets={'sadd':'$r->sAdd($key,"a","b")','saddarray':'$r->sAddArray($key,["a","b"])',
'scard':'$r->sCard($key)','sismember':'$r->sIsMember($key,"a")',
'smismember':'$r->sMisMember($key,"a","missing")',
'srem':'$r->sRem($key,"a","missing")','spop':'$r->sPop($key)',
'srandmember':'$r->sRandMember($key)','smembers':'$r->sMembers($key)',
'smove':'$r->sMove($key,$p."dst","a")'}
for name,call in sets.items():
    case(name,f'''foreach (["present","missing","wrong"] as $kind) {{
$key=$p.$kind;if ($kind==="present") {{$r->sAdd($key,"a");}}
if ($kind==="wrong") {{$r->set($key,"wrong");}}
check(fn()=>{call});check(fn()=>$r->getLastError());$r->clearLastError();}}''')
for name in ['sDiff','sInter','sUnion','sDiffStore','sInterStore','sUnionStore']:
    call = f'$r->{name}('+('$p."dst",' if name.endswith('Store') else '')+'$p."a",$p."b")'
    case(name.lower(), f'''$r->sAdd($p."a","a");$r->sAdd($p."b","a");
check(fn()=>{call});$r->del($p."b");check(fn()=>{call});''')
case('sintercard','''$r->sAdd($p."a","a","b");$r->sAdd($p."b","a");
foreach ([-1,0,1,2] as $limit) {check(fn()=>$r->sInterCard([$p."a",$p."b"],$limit));}''')
zsets={
'zadd':'$r->zAdd($key,["NX"],2,"b")','zcard':'$r->zCard($key)',
'zcount':'$r->zCount($key,"-inf","+inf")','zscore':'$r->zScore($key,"a")',
'zrank':'$r->zRank($key,"a")','zrevrank':'$r->zRevRank($key,"a")',
'zmscore':'$r->zMscore($key,"a","missing")','zincrby':'$r->zIncrBy($key,0.5,"a")',
'zrange':'$r->zRange($key,0,-1,true)','zrevrange':'$r->zRevRange($key,0,-1,true)',
'zrangebyscore':'$r->zRangeByScore($key,"-inf","+inf",["withscores"=>true,"limit"=>[0,2]])',
'zrevrangebyscore':'$r->zRevRangeByScore($key,"+inf","-inf",["withscores"=>true])',
'zrangebylex':'$r->zRangeByLex($key,"-","+",0,10)',
'zrevrangebylex':'$r->zRevRangeByLex($key,"+","-",0,10)',
'zlexcount':'$r->zLexCount($key,"-","+")',
'zrem':'$r->zRem($key,"a","missing")',
'zremrangebyrank':'$r->zRemRangeByRank($key,0,-1)',
'zremrangebyscore':'$r->zRemRangeByScore($key,"-inf","+inf")',
'zremrangebylex':'$r->zRemRangeByLex($key,"-","+")',
'zpopmin':'$r->zPopMin($key,2)','zpopmax':'$r->zPopMax($key,2)',
'zrandmember':'$r->zRandMember($key,["count"=>1,"withscores"=>true])',
'zmpop':'$r->zmpop([$key],"MIN",1)','bzmpop':'$r->bzmpop(0.01,[$key],"MAX",1)',
'bzpopmin':'$r->bzPopMin([$key],1)','bzpopmax':'$r->bzPopMax([$key],1)',
}
for name,call in zsets.items():
    case(name,f'''foreach (["present","missing","wrong"] as $kind) {{
$key=$p.$kind;if ($kind==="present") {{$r->zAdd($key,1.5,"a");}}
if ($kind==="wrong") {{$r->set($key,"wrong");}}
check(fn()=>{call});check(fn()=>$r->getLastError());$r->clearLastError();}}''')
for name in ['zinter','zunion','zdiff','zinterstore','zunionstore','zdiffstore','zintercard']:
    args = ('$p."dst",' if name.endswith('store') else '')+'[$p."a",$p."b"]'
    case(name,f'''$r->zAdd($p."a",1,"a");$r->zAdd($p."b",2,"a");
check(fn()=>$r->{name}({args}));$r->del($p."b");check(fn()=>$r->{name}({args}));''')
case('zrangestore','''$r->zAdd($p."a",1,"a",2,"b");
check(fn()=>$r->zrangestore($p."b",$p."a","0","-1"));
check(fn()=>$r->zRange($p."b",0,-1,true));''')
for name in ['pfadd','pfcount','pfmerge']:
    call={'pfadd':'$r->pfAdd($p."a",["a","b","a"])',
          'pfcount':'$r->pfCount([$p."a",$p."b"])',
          'pfmerge':'$r->pfMerge($p."b",[$p."a"])'}[name]
    case(name,f'''check(fn()=>{call});$r->pfAdd($p."a",["a","b"]);
check(fn()=>{call});check(fn()=>$r->pfCount($p."b"));''')
for name,call in {'xadd':'$r->xAdd($key,"2-0",["f"=>"v\\0"])',
'xrange':'$r->xRange($key,"-","+",10)','xrevrange':'$r->xRevRange($key,"+","-",10)',
'xlen':'$r->xLen($key)','xdel':'$r->xDel($key,["1-0"])',
'xtrim':'$r->xTrim($key,"1")','xread':'$r->xRead([$key=>"0-0"],10)',
'xgroup':'$r->xGroup("CREATE",$key,"g","0",true)'}.items():
    case(name,f'''foreach (["present","missing","wrong"] as $kind) {{
$key=$p.$kind;if ($kind==="present") {{$r->xAdd($key,"1-0",["f"=>"v"]);}}
if ($kind==="wrong") {{$r->set($key,"wrong");}}
check(fn()=>{call});check(fn()=>$r->getLastError());$r->clearLastError();}}''')
for name,call in {'geoadd':'$r->geoAdd($p."g",13.361389,38.115556,"Palermo")',
'geodist':'$r->geoDist($p."g","Palermo","Catania","km")',
'geohash':'$r->geoHash($p."g","Palermo","missing")',
'geopos':'$r->geoPos($p."g","Palermo","missing")',
'georadius':'$r->geoRadius($p."g",13.361389,38.115556,1,"km")',
'geosearch':'$r->geoSearch($p."g","Palermo",1,"km")',
'geosearchstore':'$r->geoSearchStore($p."dst",$p."g","Palermo",1,"km")'}.items():
    case(name,f'''$r->geoAdd($p."g",13.361389,38.115556,"Palermo",15.087269,37.502669,"Catania");
check(fn()=>{call});''')
case('eval','''check(fn()=>$r->eval('return {KEYS[1],ARGV[1],false}',[$p."k","a\\0"],1));
check(fn()=>$r->eval('return 42'));check(fn()=>$r->eval('return redis.call("GET",KEYS[1])',[$p."missing"],1));''')
case('evalsha','''$sha=$r->script("LOAD","return ARGV[1]");
check(fn()=>$r->evalSha($sha,["abc"],0));check(fn()=>$r->script("EXISTS",$sha));''')
case('rawcommand','''check(fn()=>$r->rawCommand("SET",$p."k","v\\0"));
check(fn()=>$r->rawCommand("GET",$p."k"));check(fn()=>$r->rawCommand("GET",$p."missing"));
check(fn()=>$r->rawCommand("PING"));''')
case('connection','''foreach (["getHost","getPort","getDBNum","getTimeout","getPersistentID","getAuth",
"isConnected","ping","getReadTimeout","getLastError","clearLastError"] as $method) {
check(fn()=>$r->$method());}
check(fn()=>$r->echo("a\\0b"));check(fn()=>$r->ping("hello"));''')
case('options','''foreach ([Redis::OPT_PREFIX,Redis::OPT_SERIALIZER,Redis::OPT_READ_TIMEOUT,
Redis::OPT_SCAN,Redis::OPT_NULL_MULTIBULK_AS_NULL,Redis::OPT_REPLY_LITERAL,Redis::OPT_COMPRESSION] as $option) {
check(fn()=>$r->getOption($option));}
check(fn()=>$r->setOption(Redis::OPT_PREFIX,$p));check(fn()=>$r->set("key","v"));
check(fn()=>$r->get("key"));check(fn()=>$r->_prefix("key"));''')
for serializer in ['NONE','JSON']:
    case('serializer_'+serializer.lower(),f'''$r->setOption(Redis::OPT_SERIALIZER,Redis::SERIALIZER_{serializer});
foreach ([null,false,true,42,1.5,"a\\0b",["a"=>[1,2]]] as $value) {{
check(fn()=>$r->set($p."k",$value));check(fn()=>$r->get($p."k"));
check(fn()=>$r->hSet($p."h","f",$value));check(fn()=>$r->hGet($p."h","f"));}}''')
for mode in ['pipeline','multi']:
    case(mode,f'''check(fn()=>$r->{mode}());check(fn()=>$r->set($p."s","v"));
check(fn()=>$r->get($p."s"));check(fn()=>$r->hSet($p."h","f","v"));
check(fn()=>$r->hGetAll($p."h"));check(fn()=>$r->rPush($p."l","a"));
check(fn()=>$r->lPop($p."l"));check(fn()=>$r->sAdd($p."s2","a"));
check(fn()=>$r->zAdd($p."z",1,"a"));check(fn()=>$r->zRange($p."z",0,-1,true));
check(fn()=>$r->xAdd($p."x","1-0",["f"=>"v"]));
check(fn()=>$r->xRange($p."x","-","+"));check(fn()=>$r->get($p."missing"));
check(fn()=>$r->hGet($p."s","wrong"));check(fn()=>$r->exec());''')
case('watch','''$other=new Redis();$other->connect("127.0.0.1",16399);$other->select(14);
$r->set($p."w","first");check(fn()=>$r->watch($p."w"));
$other->set($p."w","other");$r->multi();$r->get($p."w");check(fn()=>$r->exec());
check(fn()=>$r->unwatch());''')
for i,(name,body) in enumerate(cases.items()):
    (scripts/f'{i:03}_{name}.php').write_text('<?php\nrequire __DIR__."/../support.php";\n'+body+'\nfinish();\n')
print(len(cases),'scripts')

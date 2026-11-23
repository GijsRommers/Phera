--TEST--
An ArrayAccess object reached through properties, for every kind of path instruction (read, isset, ??, empty, write, append, compound assignment, increment, unset, a reference, sends by value and by reference), each call logged, three rounds.
--FILE--
<?php
// An ArrayAccess object reached through properties, for every kind of path
// instruction: read, isset, ??, empty, write, append, compound assignment,
// increment, unset, reference, and a send by value and by reference. Each
// call logs what the object was asked.

class Store implements ArrayAccess {
    public array $data = ['a' => 1, 'list' => [1, 2], 'nested' => null];
    public array $log = [];
    function offsetExists(mixed $o): bool { $this->log[] = "exists($o)"; return isset($this->data[$o]); }
    function offsetGet(mixed $o): mixed { $this->log[] = "get($o)"; return $this->data[$o] ?? null; }
    function offsetSet(mixed $o, mixed $v): void { $this->log[] = 'set(' . ($o ?? 'null') . ')'; if ($o === null) { $this->data[] = $v; } else { $this->data[$o] = $v; } }
    function offsetUnset(mixed $o): void { $this->log[] = "unset($o)"; unset($this->data[$o]); }
    function flush(): string { $line = implode(' ', $this->log); $this->log = []; return $line; }
}

class Holder { public $store; public $inner; public array $plain = ['k' => 'plain']; }

function by_value($v) { return json_encode($v); }
function by_reference(&$v) { $v = 'changed'; return 'ref'; }

class Runner {
    public $h;
    function __construct() { $this->h = new Holder; $this->h->store = new Store; $this->h->inner = new Holder; $this->h->inner->store = new Store; }
    function run() {
        $s = $this->h->store;
        echo json_encode([$this->h->store['a'], isset($this->h->store['a']), isset($this->h->store['zz']), $this->h->store['zz'] ?? 'dflt', empty($this->h->store['a'])]), ' ', $s->flush(), "\n";
        echo json_encode($this->h->store['list'][1]), ' ', json_encode(isset($this->h->store['list'][5])), ' ', $s->flush(), "\n";
        $this->h->store['b'] = 2;
        $this->h->store[] = 'appended';
        $this->h->store['a'] += 10;
        $this->h->store['n'] = 0;
        $x = $this->h->store['n']++;
        echo $x, ' ', $s->flush(), "\n";
        unset($this->h->store['b']);
        echo $s->flush(), "\n";
        echo by_value($this->h->store['a']), ' ', $s->flush(), "\n";
        try { echo by_reference($this->h->store['a']), ' '; } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), ' '; }
        echo $s->flush(), "\n";
        try { $alias = &$this->h->store['a']; $alias = 99; } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), ' '; }
        $this->h->store['a'] -= 1;
        $this->h->store['list'][] = 3;
        $this->h->store['list'][0] = 'x';
        echo $s->flush(), "\n";
        $r = $this->h->inner->store['a'];
        $this->h->inner->store['deep'] = 'd';
        echo $r, ' ', $this->h->inner->store->flush(), "\n";
        echo $this->h->plain['k'], "\n";
        $local = ['store' => $s];
        $local['store']['viaLocal'] = 'v';
        echo $local['store']['viaLocal'], ' ', $s->flush(), "\n";
        echo json_encode($s->data), "\n";
    }
}

$runner = new Runner;
for ($i = 0; $i < 3; $i++) {
    echo "--- round $i\n";
    $runner->run();
}

--EXPECTF--
--- round 0
[1,true,false,"dflt",false] get(a) exists(a) exists(zz) exists(zz) exists(a) get(a)
2 false get(list) exists(list) get(list)

Notice: Indirect modification of overloaded element of Store has no effect in %s on line 33
0 set(b) set(null) get(a) set(a) set(n) get(n)
unset(b)
11 get(a)

Notice: Indirect modification of overloaded element of Store has no effect in %s on line 38
ref get(a)

Notice: Indirect modification of overloaded element of Store has no effect in %s on line 40

Notice: Indirect modification of overloaded element of Store has no effect in %s on line 42

Notice: Indirect modification of overloaded element of Store has no effect in %s on line 43
get(a) get(a) set(a) get(list) get(list)
1 get(a) set(deep)
plain
v set(viaLocal) get(viaLocal)
{"a":10,"list":[1,2],"nested":null,"0":"appended","n":0,"viaLocal":"v"}
--- round 1
[10,true,false,"dflt",false] get(a) exists(a) exists(zz) exists(zz) exists(a) get(a)
2 false get(list) exists(list) get(list)

Notice: Indirect modification of overloaded element of Store has no effect in %s on line 33
0 set(b) set(null) get(a) set(a) set(n) get(n)
unset(b)
20 get(a)

Notice: Indirect modification of overloaded element of Store has no effect in %s on line 38
ref get(a)

Notice: Indirect modification of overloaded element of Store has no effect in %s on line 40

Notice: Indirect modification of overloaded element of Store has no effect in %s on line 42

Notice: Indirect modification of overloaded element of Store has no effect in %s on line 43
get(a) get(a) set(a) get(list) get(list)
1 get(a) set(deep)
plain
v set(viaLocal) get(viaLocal)
{"a":19,"list":[1,2],"nested":null,"0":"appended","n":0,"viaLocal":"v","1":"appended"}
--- round 2
[19,true,false,"dflt",false] get(a) exists(a) exists(zz) exists(zz) exists(a) get(a)
2 false get(list) exists(list) get(list)

Notice: Indirect modification of overloaded element of Store has no effect in %s on line 33
0 set(b) set(null) get(a) set(a) set(n) get(n)
unset(b)
29 get(a)

Notice: Indirect modification of overloaded element of Store has no effect in %s on line 38
ref get(a)

Notice: Indirect modification of overloaded element of Store has no effect in %s on line 40

Notice: Indirect modification of overloaded element of Store has no effect in %s on line 42

Notice: Indirect modification of overloaded element of Store has no effect in %s on line 43
get(a) get(a) set(a) get(list) get(list)
1 get(a) set(deep)
plain
v set(viaLocal) get(viaLocal)
{"a":28,"list":[1,2],"nested":null,"0":"appended","n":0,"viaLocal":"v","1":"appended","2":"appended"}

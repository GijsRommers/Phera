--TEST--
Realistic cache, queue and configuration records match PHP bytes and dumps
--FILE--
<?php
class CacheItem {
    public function __construct(private string $id, protected array $tags, public int $ttl) {}
}
class JobPayload {
    private array $data;
    public function __construct(array $data) { $this->data = $data; }
    public function __serialize(): array { return ['payload' => $this->data, 'version' => 2]; }
    public function __unserialize(array $data): void { $this->data = $data['payload']; }
}
$node = new stdClass;
$node->name = 'root';
$node->child = new stdClass;
$node->child->parent = $node;
$shared = 'shared-secret';
$previous = new ErrorException('The API timed out', 503, E_USER_WARNING, '/srv/Gateway.php', 21);
$failure = new ErrorException('The job failed', 1, E_USER_ERROR, '/srv/Runner.php', 150, $previous);
$cases = [
    'config' => ['cache' => ['driver' => 'redis', 'ttl' => 300], 'debug' => false, 'threshold' => 0.125],
    'cycle' => $node,
    'cache items' => [new CacheItem('a-17', ['tenant:4'], 60), new CacheItem('b-21', [], 150)],
    'exception' => $failure,
    'scheduled time' => new DateTimeImmutable('2026-10-25 03:00:00.125000', new DateTimeZone('Europe/Brussels')),
    'shared aliases' => ['connection' => &$shared, 'fallback' => &$shared],
    'queue payload' => new JobPayload(['account' => 10, 'action' => 'notify', 'attempts' => 2]),
    'headers' => ['content-type' => 'text/plain', 'set-cookie' => ['token=a', 'lang=nl']],
    'routes' => ['home' => ['path' => '/', 'methods' => ['GET']], 'list' => ['path' => '/items', 'methods' => ['GET', 'HEAD']]],
    'session' => ['id' => 'f088ad87', 'roles' => ['ROLE_USER'], 'csrf' => 'd15c1e4a', 'remember' => null],
];
$encodings = [
    '613a333a7b733a353a226361636865223b613a323a7b733a363a22647269766572223b733a353a227265646973223b733a333a2274746c223b693a3330303b7d733a353a226465627567223b623a303b733a393a227468726573686f6c64223b643a302e3132353b7d',
    '4f3a383a22737464436c617373223a323a7b733a343a226e616d65223b733a343a22726f6f74223b733a353a226368696c64223b4f3a383a22737464436c617373223a313a7b733a363a22706172656e74223b723a313b7d7d',
    '613a323a7b693a303b4f3a393a2243616368654974656d223a333a7b733a31333a220043616368654974656d006964223b733a343a22612d3137223b733a373a22002a0074616773223b613a313a7b693a303b733a383a2274656e616e743a34223b7d733a333a2274746c223b693a36303b7d693a313b4f3a393a2243616368654974656d223a333a7b733a31333a220043616368654974656d006964223b733a343a22622d3231223b733a373a22002a0074616773223b613a303a7b7d733a333a2274746c223b693a3135303b7d7d',
    '4f3a31343a224572726f72457863657074696f6e223a383a7b733a31303a22002a006d657373616765223b733a31343a22546865206a6f62206661696c6564223b733a31373a2200457863657074696f6e00737472696e67223b733a303a22223b733a373a22002a00636f6465223b693a313b733a373a22002a0066696c65223b733a31353a222f7372762f52756e6e65722e706870223b733a373a22002a006c696e65223b693a3135303b733a31363a2200457863657074696f6e007472616365223b613a303a7b7d733a31393a2200457863657074696f6e0070726576696f7573223b4f3a31343a224572726f72457863657074696f6e223a383a7b733a31303a22002a006d657373616765223b733a31373a22546865204150492074696d6564206f7574223b733a31373a2200457863657074696f6e00737472696e67223b733a303a22223b733a373a22002a00636f6465223b693a3530333b733a373a22002a0066696c65223b733a31363a222f7372762f476174657761792e706870223b733a373a22002a006c696e65223b693a32313b733a31363a2200457863657074696f6e007472616365223b613a303a7b7d733a31393a2200457863657074696f6e0070726576696f7573223b4e3b733a31313a22002a007365766572697479223b693a3531323b7d733a31313a22002a007365766572697479223b693a3235363b7d',
    '4f3a31373a224461746554696d65496d6d757461626c65223a333a7b733a343a2264617465223b733a32363a22323032362d31302d32352030333a30303a30302e313235303030223b733a31333a2274696d657a6f6e655f74797065223b693a333b733a383a2274696d657a6f6e65223b733a31353a224575726f70652f4272757373656c73223b7d',
    '613a323a7b733a31303a22636f6e6e656374696f6e223b733a31333a227368617265642d736563726574223b733a383a2266616c6c6261636b223b523a323b7d',
    '4f3a31303a224a6f625061796c6f6164223a323a7b733a373a227061796c6f6164223b613a333a7b733a373a226163636f756e74223b693a31303b733a363a22616374696f6e223b733a363a226e6f74696679223b733a383a22617474656d707473223b693a323b7d733a373a2276657273696f6e223b693a323b7d',
    '613a323a7b733a31323a22636f6e74656e742d74797065223b733a31303a22746578742f706c61696e223b733a31303a227365742d636f6f6b6965223b613a323a7b693a303b733a373a22746f6b656e3d61223b693a313b733a373a226c616e673d6e6c223b7d7d',
    '613a323a7b733a343a22686f6d65223b613a323a7b733a343a2270617468223b733a313a222f223b733a373a226d6574686f6473223b613a313a7b693a303b733a333a22474554223b7d7d733a343a226c697374223b613a323a7b733a343a2270617468223b733a363a222f6974656d73223b733a373a226d6574686f6473223b613a323a7b693a303b733a333a22474554223b693a313b733a343a2248454144223b7d7d7d',
    '613a343a7b733a323a226964223b733a383a226630383861643837223b733a353a22726f6c6573223b613a313a7b693a303b733a393a22524f4c455f55534552223b7d733a343a2263737266223b733a383a226431356331653461223b733a383a2272656d656d626572223b4e3b7d',
];
$index = 0;
foreach ($cases as $label => $value) {
    echo $label, "\n";
    $bytes = hex2bin($encodings[$index++]);
    var_dump(serialize($value) === $bytes);
    var_dump(unserialize($bytes));
}
--EXPECT--
config
bool(true)
array(3) {
  ["cache"]=>
  array(2) {
    ["driver"]=>
    string(5) "redis"
    ["ttl"]=>
    int(300)
  }
  ["debug"]=>
  bool(false)
  ["threshold"]=>
  float(0.125)
}
cycle
bool(true)
object(stdClass)#9 (2) {
  ["name"]=>
  string(4) "root"
  ["child"]=>
  object(stdClass)#10 (1) {
    ["parent"]=>
    *RECURSION*
  }
}
cache items
bool(true)
array(2) {
  [0]=>
  object(CacheItem)#11 (3) {
    ["id":"CacheItem":private]=>
    string(4) "a-17"
    ["tags":protected]=>
    array(1) {
      [0]=>
      string(8) "tenant:4"
    }
    ["ttl"]=>
    int(60)
  }
  [1]=>
  object(CacheItem)#12 (3) {
    ["id":"CacheItem":private]=>
    string(4) "b-21"
    ["tags":protected]=>
    array(0) {
    }
    ["ttl"]=>
    int(150)
  }
}
exception
bool(true)
object(ErrorException)#12 (8) {
  ["message":protected]=>
  string(14) "The job failed"
  ["string":"Exception":private]=>
  string(0) ""
  ["code":protected]=>
  int(1)
  ["file":protected]=>
  string(15) "/srv/Runner.php"
  ["line":protected]=>
  int(150)
  ["trace":"Exception":private]=>
  array(0) {
  }
  ["previous":"Exception":private]=>
  object(ErrorException)#11 (8) {
    ["message":protected]=>
    string(17) "The API timed out"
    ["string":"Exception":private]=>
    string(0) ""
    ["code":protected]=>
    int(503)
    ["file":protected]=>
    string(16) "/srv/Gateway.php"
    ["line":protected]=>
    int(21)
    ["trace":"Exception":private]=>
    array(0) {
    }
    ["previous":"Exception":private]=>
    NULL
    ["severity":protected]=>
    int(512)
  }
  ["severity":protected]=>
  int(256)
}
scheduled time
bool(true)
object(DateTimeImmutable)#12 (3) {
  ["date"]=>
  string(26) "2026-10-25 03:00:00.125000"
  ["timezone_type"]=>
  int(3)
  ["timezone"]=>
  string(15) "Europe/Brussels"
}
shared aliases
bool(true)
array(2) {
  ["connection"]=>
  &string(13) "shared-secret"
  ["fallback"]=>
  &string(13) "shared-secret"
}
queue payload
bool(true)
object(JobPayload)#12 (1) {
  ["data":"JobPayload":private]=>
  array(3) {
    ["account"]=>
    int(10)
    ["action"]=>
    string(6) "notify"
    ["attempts"]=>
    int(2)
  }
}
headers
bool(true)
array(2) {
  ["content-type"]=>
  string(10) "text/plain"
  ["set-cookie"]=>
  array(2) {
    [0]=>
    string(7) "token=a"
    [1]=>
    string(7) "lang=nl"
  }
}
routes
bool(true)
array(2) {
  ["home"]=>
  array(2) {
    ["path"]=>
    string(1) "/"
    ["methods"]=>
    array(1) {
      [0]=>
      string(3) "GET"
    }
  }
  ["list"]=>
  array(2) {
    ["path"]=>
    string(6) "/items"
    ["methods"]=>
    array(2) {
      [0]=>
      string(3) "GET"
      [1]=>
      string(4) "HEAD"
    }
  }
}
session
bool(true)
array(4) {
  ["id"]=>
  string(8) "f088ad87"
  ["roles"]=>
  array(1) {
    [0]=>
    string(9) "ROLE_USER"
  }
  ["csrf"]=>
  string(8) "d15c1e4a"
  ["remember"]=>
  NULL
}

--EXPECTF--
object(JsonException)#1 (7) {
  ["message":protected]=>
  string(30) "Syntax error near location 1:2"
  ["string":"Exception":private]=>
  string(0) ""
  ["code":protected]=>
  int(4)
  ["file":protected]=>
  string(%d) "%s"
  ["line":protected]=>
  int(4)
  ["trace":"Exception":private]=>
  array(1) {
    [0]=>
    array(4) {
      ["file"]=>
      string(%d) "%s"
      ["line"]=>
      int(4)
      ["function"]=>
      string(11) "json_decode"
      ["args"]=>
      array(4) {
        [0]=>
        string(1) "{"
        [1]=>
        bool(false)
        [2]=>
        int(512)
        [3]=>
        int(4194304)
      }
    }
  }
  ["previous":"Exception":private]=>
  NULL
}

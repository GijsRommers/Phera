--TEST--
Symfony-style strict save handler around the native files backend
--FILE--
<?php
ob_start();
class Store extends SessionHandler implements SessionUpdateTimestampHandlerInterface {
    private string $id = "";
    private string $data = "";
    public function validateId(string $id): bool {
        $data = $this->read($id);
        return $data !== "" && $data !== false;
    }
    public function read(string $id): string|false {
        if ($this->id !== $id) {
            $this->id = $id;
            $this->data = parent::read($id) ?: "";
        }
        return $this->data;
    }
    public function create_sid(): string { return session_create_id(); }
    public function updateTimestamp(string $id, string $data): bool { return $this->write($id, $data); }
}
$dir = tempnam(sys_get_temp_dir(), "phera-storage-");
unlink($dir);
mkdir($dir);
session_save_path($dir);
ini_set("session.gc_probability", 0);
ini_set("session.use_strict_mode", 1);
session_cache_limiter("");
session_register_shutdown();
session_set_save_handler(new Store(), false);
session_id("symfony");
file_put_contents($dir . "/sess_symfony", 'cart|i:2;');
var_dump(session_start(), $_SESSION);
$_SESSION["cart"]++;
session_write_close();
echo file_get_contents($dir . "/sess_symfony"), "\n";
unlink($dir . "/sess_symfony");
rmdir($dir);
?>
--EXPECT--
bool(true)
array(1) {
  ["cart"]=>
  int(2)
}
cart|i:3;

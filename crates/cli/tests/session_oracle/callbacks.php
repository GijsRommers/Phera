<?php
ob_start();
class V implements SessionHandlerInterface, SessionIdInterface, SessionUpdateTimestampHandlerInterface {
    public array $trace = [];
    public function __construct(public string $target, public string $mode) {}
    private function run($method, $default) {
        $this->trace[] = $method;
        if ($this->target !== $method) return $default;
        return match ($this->mode) {
            'throw' => throw new RuntimeException('callback'),
            'false' => false,
            'true' => true,
            'array' => [],
            'null' => null,
            'zero' => 0,
            'negative' => -1,
        };
    }
    #[ReturnTypeWillChange] public function open($path, $name) {return $this->run('open', true);}
    #[ReturnTypeWillChange] public function read($id) {return $this->run('read', 'key|s:2:"ok";');}
    #[ReturnTypeWillChange] public function close() {return $this->run('close', true);}
    #[ReturnTypeWillChange] public function write($id, $data) {return $this->run('write', true);}
    #[ReturnTypeWillChange] public function destroy($id) {return $this->run('destroy', true);}
    #[ReturnTypeWillChange] public function gc($max_lifetime) {return $this->run('gc', 0);}
    #[ReturnTypeWillChange] public function create_sid() {return $this->run('create', 'test');}
    #[ReturnTypeWillChange] public function validateId($id) {return $this->run('validate', true);}
    #[ReturnTypeWillChange] public function updateTimestamp($id,$data) {return $this->run('touch', true);}
}
session_save_path('');
ini_set('session.serialize_handler', 'php');
ini_set('session.lazy_write', 1);
ini_set('session.gc_probability', 0);
ini_set('session.use_cookies', 0);
session_cache_limiter('');
foreach (['open', 'read', 'close', 'write', 'destroy', 'gc', 'create', 'validate', 'touch'] as $target) {
    foreach (['throw', 'false', 'true', 'array', 'null', 'zero', 'negative'] as $mode) {
        $warnings = [];
        $v = new V($target, $mode);
        session_set_save_handler($v, false);
        ini_set('session.use_strict_mode', $target === 'validate');
        session_id($target === 'create' ? '' : 'test');
        set_error_handler(function($level, $text) use (&$warnings) {
            $warnings[] = [$level, $text];
            return true;
        });
        $state = [];
        try {
            $state[] = session_start();
            if (session_status() === PHP_SESSION_ACTIVE) {
                if ($target === 'destroy') { $state[] = session_destroy(); }
                elseif ($target === 'gc') { $state[] = session_gc(); }
                else {
                    if ($target !== 'touch') $_SESSION['other'] = true;
                    $state[] = session_write_close();
                }
            }
        } catch (Throwable $e) {
            $state[] = [$e::class, $e->getMessage()];
        }
        $state[] = session_status();
        $state[] = session_id();
        restore_error_handler();
        echo json_encode([$target, $mode, $v->trace, $state, $warnings]), "\n";
        if (session_status() === PHP_SESSION_ACTIVE) @session_abort();
    }
}

<?php
class CookieOption {
    public function __construct(private string $value) {}

    public function __toString(): string {
        $GLOBALS['conversions'][] = $this->value;
        if ($this->value === 'throw') {
            throw new LogicException('cookie option');
        }
        return $this->value;
    }
}

header_remove('X-Powered-By');
$results = [];
foreach (['setcookie', 'setrawcookie'] as $function) {
    foreach ([
        ['path' => new CookieOption('/api'), 'domain' => new CookieOption('example.test'),
            'secure' => true, 'samesite' => new CookieOption('Strict')],
        ['PATH' => new CookieOption('/old'), 'path' => new CookieOption('/new')],
        ['path' => new CookieOption('bad;path')],
        ['path' => new CookieOption('throw'), 'domain' => 'example.test',
            'samesite' => new CookieOption('skipped')],
    ] as $options) {
        header_remove();
        $GLOBALS['conversions'] = [];
        try {
            $result = $function('session', 'value', $options);
        } catch (Throwable $exception) {
            $result = [get_class($exception), $exception->getMessage()];
        }
        $results[] = json_encode([$function, $result, $GLOBALS['conversions'], headers_list()]);
    }
}
header_remove();
header('Content-Type: application/json');
echo implode("\n", $results);

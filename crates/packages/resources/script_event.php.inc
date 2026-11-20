namespace Composer\Script;

class Event
{
    public function __construct(
        private string $name,
        private string $vendorDir,
        private bool $devMode,
    ) {}

    public function getName(): string
    {
        return $this->name;
    }

    public function isDevMode(): bool
    {
        return $this->devMode;
    }

    public function getComposer(): object
    {
        return new class($this->vendorDir) {
            public function __construct(private string $vendorDir) {}

            public function getConfig(): object
            {
                return new class($this->vendorDir) {
                    public function __construct(private string $vendorDir) {}

                    public function get(string $key): mixed
                    {
                        return match ($key) {
                            'vendor-dir' => $this->vendorDir,
                            default => throw new \LogicException("Unsupported Composer config key: $key"),
                        };
                    }
                };
            }
        };
    }
}

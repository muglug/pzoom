<?php
final class Cfg2 {
    /** @var array<callable-string, bool> */
    private array $predefined_functions = [];

    /**
     * @return array<callable-string, bool>
     */
    public function getPredefinedFunctions(): array
    {
        return $this->predefined_functions;
    }

    public function collect(): void
    {
        $defined_functions = get_defined_functions();
        foreach ($defined_functions['user'] as $function_name) {
            $this->predefined_functions[$function_name] = true;
        }
    }
}

function reflectCallable(callable $callable): void {
    if (\is_array($callable)) {
        echo 'arr';
    } elseif ($callable instanceof \Closure || \is_string($callable)) {
        new \ReflectionFunction($callable);
    }
}

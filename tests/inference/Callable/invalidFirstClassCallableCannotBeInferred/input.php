<?php
/**
 * @template T1
 */
final class App
{
    /**
     * @param T1 $param1
     */
    public function __construct(
        private readonly mixed $param1,
    ) {}

    /**
     * @template T2
     * @param callable(T1): T2 $callback
     * @return T2
     */
    public function run(callable $callback): mixed
    {
        return $callback($this->param1);
    }
}

/**
 * @template P1 of int|float
 * @param P1 $param1
 * @return array{param1: P1}
 */
function appHandler(mixed $param1): array
{
    return ["param1" => $param1];
}

$result = (new App(param1: [42]))->run(appHandler(...));

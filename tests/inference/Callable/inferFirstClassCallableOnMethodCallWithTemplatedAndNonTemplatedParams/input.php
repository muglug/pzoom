<?php
/**
 * @template T1
 * @template T2
 */
final class App
{
    /**
     * @param T1 $param1
     * @param T2 $param2
     */
    public function __construct(
        private readonly mixed $param1,
        private readonly mixed $param2,
    ) {
    }

    /**
     * @template T3
     * @param callable(T1, T2): T3 $callback
     * @return T3
     */
    public function run(callable $callback): mixed
    {
        return $callback($this->param1, $this->param2);
    }
}

/**
 * @template T of int|float
 * @param T $param2
 * @return array{param1: int, param2: T}
 */
function appHandler1(int $param1, int|float $param2): array
{
    return ["param1" => $param1, "param2" => $param2];
}

/**
 * @template T of int|float
 * @param T $param1
 * @return array{param1: T, param2: int}
 */
function appHandler2(int|float $param1, int $param2): array
{
    return ["param1" => $param1, "param2" => $param2];
}

/**
 * @return array{param1: int, param2: int}
 */
function appHandler3(int $param1, int $param2): array
{
    return ["param1" => $param1, "param2" => $param2];
}

$app = new App(param1: 42, param2: 42);

$result1 = $app->run(appHandler1(...));
$result2 = $app->run(appHandler2(...));
$result3 = $app->run(appHandler3(...));

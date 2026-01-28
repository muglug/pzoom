<?php
/**
 * @template T
 */
class Promise {
    /** @var T */
    private $t;

    /** @param T $t */
    public function __construct($t) {
        $this->t = $t;
    }
}

/**
 * @template T
 * @extends Promise<T>
 */
class Success extends Promise {}

/**
 * @template TReturn
 * @template TPromise
 *
 * @template T as Promise<TPromise>|TReturn
 *
 * @param callable(): T $callback
 *
 * @return Promise
 * @psalm-return (T is Promise ? Promise<TPromise> : Promise<TReturn>)
 */
function call(callable $callback): Promise {
    $result = $callback();

    if ($result instanceof Promise) {
        return $result;
    }

    return new Promise($result);
}

$ret_int_promise = function (): Promise {
    return new Success(9);
};

$c1 = call($ret_int_promise);

$c2 = call(function (): int {
    return 42;
});
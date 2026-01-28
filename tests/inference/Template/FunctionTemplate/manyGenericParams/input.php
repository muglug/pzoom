<?php
/**
 * @template TArg1
 * @template TArg2
 * @template TRes
 *
 * @psalm-param Closure(TArg1, TArg2): TRes $func
 * @psalm-param TArg1 $arg1
 *
 * @psalm-return Closure(TArg2): TRes
 */
function partial(Closure $func, $arg1): Closure {
    return fn($arg2) => $func($arg1, $arg2);
}

/**
 * @template TArg1
 * @template TArg2
 * @template TRes
 *
 * @template T as (Closure(): TRes | Closure(TArg1): TRes | Closure(TArg1, TArg2): TRes)
 *
 * @psalm-param T $fn
 * @psalm-param TArg1 $arg
 */
function foo(Closure $fn, $arg): void {
    $a = partial($fn, $arg);
}
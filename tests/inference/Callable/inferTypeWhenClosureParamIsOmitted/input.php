<?php
/**
 * @template A
 * @template B
 * @param A $a
 * @param callable(A): B $ab
 * @return B
 */
function pipe(mixed $a, callable $ab): mixed
{
    return $ab($a);
}
/**
 * @template A
 * @param callable(A): void $callback
 * @return Closure(list<A>): list<A>
 */
function iterate(callable $callback): Closure
{
    return function(array $list) use ($callback) {
        foreach ($list as $item) {
            $callback($item);
        }
        return $list;
    };
}
$result1 = pipe(
    [1, 2, 3],
    iterate(fn($i) => print_r($i)),
);
$result2 = pipe(
    [1, 2, 3],
    iterate(fn() => print_r("noop")),
);

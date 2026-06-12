<?php
/**
 * @template T
 * @param T $value
 * @return T
 */
function id(mixed $value): mixed
{
    return $value;
}

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
 * @template B
 * @param callable(A): B $callback
 * @return Closure(list<A>): list<B>
 */
function map(callable $callback): Closure
{
    return fn($array) => array_map($callback, $array);
}

/**
 * @return list<int>
 */
function getNums(): array
{
    return [];
}

/**
 * @template T of float|int
 */
final class ObjectNum
{
    /**
     * @psalm-param T $value
     */
    public function __construct(
        public readonly float|int $value,
    ) {}
}

/**
 * @return list<ObjectNum<int>>
 */
function getObjectNums(): array
{
    return [];
}

$id = pipe(getNums(), id(...));
$wrapped_id = pipe(getNums(), map(id(...)));
$id_nested = pipe(getObjectNums(), map(id(...)));
$id_nested_simple = pipe(getObjectNums(), id(...));

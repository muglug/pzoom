<?php
/**
 * @template A
 * @template B
 * @param A $a
 * @param callable(A): B $ab
 * @return B
 */
function pipe($a, callable $ab)
{
    return $ab($a);
}
/**
 * @template A
 * @psalm-immutable
 */
final class Container
{
    /** @param A $value */
    public function __construct(
        public readonly mixed $value,
    ) {}
}
/**
 * @template A
 * @param Container<A> $container
 * @return A
 */
function unwrap(Container $container)
{
    return $container->value;
}
$result = pipe(
    new Container(42),
    unwrap(...),
);

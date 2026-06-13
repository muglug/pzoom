<?php
final class State
{}

interface Foo
{}

function type(string ...$_p): void {}

/**
 * @template T
 */
final class AlmostFooMap
{
    /**
     * @param callable(State):(T&Foo) $closure
     */
    public function __construct(callable $closure)
    {
        type($closure);
    }
}

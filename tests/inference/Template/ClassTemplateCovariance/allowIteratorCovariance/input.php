<?php
/**
 * @template-covariant T
 */
interface ITraversable
{
    /** @psalm-return Traversable<T> */
    public function foo(): Traversable;
}

/**
 * @template-covariant T
 */
interface IArray
{
    /** @psalm-return array<T> */
    public function foo(): array;
}

/**
 * @template-covariant T
 */
interface IIterable
{
    /** @psalm-return iterable<T> */
    public function foo(): iterable;
}
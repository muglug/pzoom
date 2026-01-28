<?php
/**
 * @template T of object
 */
final class A {
    /**
     * @psalm-var ?callable(T): bool
     */
    public $filter;
}

/** @psalm-var A<A> */
$a = new A();

if (null !== $a->filter) {}
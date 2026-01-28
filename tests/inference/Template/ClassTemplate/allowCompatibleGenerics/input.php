<?php
/** @template T of object */
interface A {}

/** @template T of object */
interface B {}

/**
 * @template T of object
 * @param A<T> $a
 * @param B<T> $b
 */
function foo(A $a, B $b): void {}

/**
 * @param A<stdClass> $a
 * @param B<stdClass> $b
 */
function bar(A $a, B $b): void {
    foo($a, $b);
}
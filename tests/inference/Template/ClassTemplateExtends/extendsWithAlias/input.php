<?php
/**
 * @template TAValue
 */
abstract class A {
    /**
     * @template TAValueNew as TAValue
     *
     * @psalm-param TAValueNew $val
     */
    abstract public function foo($val): void;
}

/**
 * @template TBValue
 * @extends A<TBValue>
 */
abstract class B extends A {
    /**
     * @template TBValueNew as TBValue
     *
     * @psalm-param TBValueNew $val
     */
    abstract public function foo($val): void;
}
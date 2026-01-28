<?php
/** @template T1 */
interface I {}

/** @template T2 */
interface J {}

/**
 * @template T3
 * @template-implements I<J<T3>>
 */
final class IC implements I {
    /** @var T3 */
    public $var;

    /** @param T3 $var */
    public function __construct($var) {
        $this->var = $var;
    }
}

/** @template T4 */
final class Container
{
    /** @var I<T4> $var */
    public I $var;

    /** @param I<T4> $var */
    public function __construct(I $var) {
        $this->var = $var;
    }
}

final class Obj {}

final class B {
    /** @return Container<J<int>> */
    public function foo(int $i): Container
    {
        $ic = new IC($i);

        $container = new Container($ic);

        return $container;
    }
}
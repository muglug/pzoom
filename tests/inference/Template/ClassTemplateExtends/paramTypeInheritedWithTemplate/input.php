<?php
/**
 * @template T1
 */
interface Container {}

/**
 * @template T2
 */
abstract class SimpleClass {
    /**
     * @psalm-param T2 $param
     */
    abstract public function foo($param): void;
}

/**
 * @template T3
 *
 * @extends SimpleClass<Container<T3>>
 */
abstract class ContainerClass extends SimpleClass {
    /**
     * @psalm-param Container<T3> $param
     */
    abstract public function foo($param): void;
}

/**
 * @extends ContainerClass<int>
 */
abstract class Complex extends ContainerClass {
    /**
     * @psalm-param Container<int> $param
     */
    abstract public function foo($param): void;
}
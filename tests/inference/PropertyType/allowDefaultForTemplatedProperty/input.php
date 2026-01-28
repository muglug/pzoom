<?php
/**
 * @template T as string|null
 */
abstract class A {
    /** @var list<T> */
    public $foo = [];
}

/**
 * @extends A<string>
 */
class AChild extends A {
    public $foo = ["hello"];
}

<?php
/**
 * @template T1
 * @template T2
 * @template T3
 */
trait MyTrait {}

class Foo {
    /**
     * @template-use MyTrait<int, int, array{
     * 	foo: mixed,
     * 	bar: mixed,
     * }>
     */
    use MyTrait;
}

class Bar {
    /**
     * @template-use MyTrait<int, string, bar>
     */
    use MyTrait;
}

<?php
/**
 * @template T
 */
interface Foo {
    /**
     * @return T
     */
    public function getItem();
}

trait FooTrait {
    public function getItem() {
        return "hello";
    }
}

/**
 * @template-implements Foo<string>
 */
class Bar implements Foo {
    use FooTrait;
}
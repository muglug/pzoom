<?php
/**
 * @template T as object
 */
abstract class Container
{
    /**
     * @param T $obj
     */
    abstract public function uri($obj) : string;
}

class Foo {}

/**
 * @template-extends Container<Foo>
 */
class FooContainer extends Container {
    /** @param Foo $obj */
    public function uri($obj) : string {
        return "hello";
    }
}
<?php
class Foo {}
class Bar {}

/**
 * @implements IteratorAggregate<int, Foo>
 */
class SomeIterator implements IteratorAggregate
{
    public function getIterator()
    {
        yield new Bar;
    }
}

<?php
class Foo {}
class Bar {}

/**
 * @implements IteratorAggregate<int, Foo>
 */
class SomeIterator implements IteratorAggregate
{
    /**
     * @return Traversable<int, Bar>
     */
    public function getIterator()
    {
        yield new Bar;
    }
}

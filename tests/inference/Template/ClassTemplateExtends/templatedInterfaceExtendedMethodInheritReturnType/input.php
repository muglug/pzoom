<?php
class Foo {}

/**
 * @template-implements IteratorAggregate<int, Foo>
 */
class SomeIterator implements IteratorAggregate
{
    public function getIterator() {
        yield new Foo;
    }
}

$i = (new SomeIterator())->getIterator();
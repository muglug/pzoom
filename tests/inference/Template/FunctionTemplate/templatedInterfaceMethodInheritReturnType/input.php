<?php
class Foo {}

/**
 * @psalm-suppress MissingTemplateParam
 */
class SomeIterator implements IteratorAggregate
{
    public function getIterator() {
        yield new Foo;
    }
}

$i = (new SomeIterator())->getIterator();

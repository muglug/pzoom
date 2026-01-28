<?php
/** @extends \ArrayObject<int, int> */
class Foo extends \ArrayObject {}

class A {
    /**
     * @var Foo<string>
     */
    public Foo $vector;

    /**
     * @param Foo<string> $v
     */
    public function __construct(Foo $v) {
        $this->vector = $v;
    }

    public function getIterator(): Iterator
    {
        yield from $this->vector;
    }
}
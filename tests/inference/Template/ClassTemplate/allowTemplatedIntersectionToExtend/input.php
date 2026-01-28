<?php
interface Foo {}

interface AlmostFoo {
    /**
     * @return Foo
     */
    public function makeFoo();
}

/**
 * @template T
 */
final class AlmostFooMap implements AlmostFoo {
    /** @var T&Foo */
    private $bar;

    /**
     * @param T&Foo $bar
     */
    public function __construct(Foo $bar)
    {
        $this->bar = $bar;
    }

    /**
     * @return T&Foo
     */
    public function makeFoo()
    {
        return $this->bar;
    }
}
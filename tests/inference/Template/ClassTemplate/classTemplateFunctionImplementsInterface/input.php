<?php
namespace A\B;

interface Foo {}

interface IFooGetter {
    /**
     * @return Foo
     */
    public function getFoo();
}

/**
 * @template T as Foo
 */
class FooGetter implements IFooGetter {
    /** @var T */
    private $t;

    /**
     * @param T $t
     */
    public function __construct(Foo $t)
    {
        $this->t = $t;
    }

    /**
     * @return T
     */
    public function getFoo()
    {
        return $this->t;
    }
}

function passFoo(Foo $f) : Foo {
    return (new FooGetter($f))->getFoo();
}
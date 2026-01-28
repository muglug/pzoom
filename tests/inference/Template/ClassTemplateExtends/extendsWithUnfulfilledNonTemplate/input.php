<?php
namespace A;

/**
 * @template T
 */
abstract class Container
{
    /**
     * @return T
     */
    public abstract function getItem();
}

class Foo
{
}

class Bar
{
}

/**
 * @template-extends Container<Bar>
 */
class BarContainer extends Container
{
    /**
     * @return Foo
     */
    public function getItem()
    {
        return new Foo();
    }
}

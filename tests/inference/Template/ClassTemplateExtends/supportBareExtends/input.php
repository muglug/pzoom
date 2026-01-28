<?php
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

/**
 * @extends Container<Foo>
 */
class FooContainer extends Container
{
    /**
     * @return Foo
     */
    public function getItem()
    {
        return new Foo();
    }
}

/**
 * @template TItem
 * @param Container<TItem> $c
 * @return TItem
 */
function getItemFromContainer(Container $c) {
    return $c->getItem();
}

$fc = new FooContainer();

$f1 = $fc->getItem();
$f2 = getItemFromContainer($fc);
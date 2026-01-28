<?php
/**
 * @template T
 */
class Container
{
    /**
     * @var T
     */
    private $v;
    /**
     * @param T $v
     */
    public function __construct($v)
    {
        $this->v = $v;
    }
    /**
     * @return T
     */
    public function getValue()
    {
        return $this->v;
    }
}

/**
 * @template T
 * @template-extends Container<T>
 */
class ChildContainer extends Container {}

/**
 * @template T
 * @template-extends ChildContainer<T>
 */
class GrandChildContainer extends ChildContainer {}

$fc = new GrandChildContainer(5);
$a = $fc->getValue();
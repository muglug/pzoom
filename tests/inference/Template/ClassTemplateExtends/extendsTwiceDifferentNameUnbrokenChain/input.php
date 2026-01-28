<?php
/**
 * @psalm-template T1
 */
class Container
{
    /**
     * @var T1
     */
    private $v;

    /**
     * @param T1 $v
     */
    public function __construct($v)
    {
        $this->v = $v;
    }

    /**
     * @return T1
     */
    public function getValue()
    {
        return $this->v;
    }
}

/**
 * @psalm-template T2
 * @extends Container<T2>
 */
class ChildContainer extends Container {}

/**
 * @psalm-template T3
 * @extends ChildContainer<T3>
 */
class GrandChildContainer extends ChildContainer {}

$fc = new GrandChildContainer(5);
$a = $fc->getValue();
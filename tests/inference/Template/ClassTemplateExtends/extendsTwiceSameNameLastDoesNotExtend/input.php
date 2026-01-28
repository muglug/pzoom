<?php
/**
 * @template T1
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
 * @template T2
 * @template-extends Container<T2>
 */
class ChildContainer extends Container {}

class GrandChildContainer extends ChildContainer {}

$fc = new GrandChildContainer(5);
$a = $fc->getValue();

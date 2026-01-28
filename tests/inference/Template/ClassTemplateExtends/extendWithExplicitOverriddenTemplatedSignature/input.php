<?php
class Obj {}

/**
 * @template T1
 */
class BaseContainer {
    /** @var T1 */
    private $t1;

    /** @param T1 $t1 */
    public function __construct($t1) {
        $this->t1 = $t1;
    }

    /**
     * @return T1
     */
    public function getValue()
    {
        return $this->t1;
    }
}

/**
 * @template T2 as Obj
 * @template-extends BaseContainer<T2>
 */
class Container extends BaseContainer {
    /** @param T2 $t2 */
    public function __construct($t2) {
        parent::__construct($t2);
    }

    /**
     * @return T2
     */
    public function getValue()
    {
        return parent::getValue();
    }
}
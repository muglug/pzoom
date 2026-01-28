<?php
class Obj {}

/**
 * @template T1
 */
class Container1 {
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
 * @template T2
 * @template-extends Container1<T2>
 */
class Container2 extends Container1 {}

/**
 * @template T3 as Obj
 * @template-extends Container2<T3>
 */
class Container3 extends Container2 {
    /** @param T3 $t3 */
    public function __construct($t3) {
        Container1::__construct($t3);
    }

    /**
     * @return T3
     */
    public function getValue()
    {
        return parent::getValue();
    }
}
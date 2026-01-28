<?php
/**
 * @template T0
 */
class Container {
    /**
     * @var T0
     */
    public $t;

    /**
     * @param T0 $t
     */
    public function __construct($t) {
        $this->t = $t;
    }
}

/**
 * @template T1 as object
 * @template-extends Container<T1>
 */
class ObjectContainer extends Container {}
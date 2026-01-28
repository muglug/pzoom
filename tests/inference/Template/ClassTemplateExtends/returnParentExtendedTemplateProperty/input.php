<?php
/**
 * @template T
 */
class Container {
    /**
     * @var T
     */
    public $t;

    /**
     * @param T $t
     */
    public function __construct($t) {
        $this->t = $t;
    }
}

/**
 * @template-extends Container<int>
 */
class IntContainer extends Container {
    public function __construct(int $i) {
        parent::__construct($i);
    }

    public function getValue() : int {
        return $this->t;
    }
}